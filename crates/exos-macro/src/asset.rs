//! Building an asset and embedding it at the point it is referenced.

use std::{
    collections::HashSet,
    env,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use exos_build::Mode;
use proc_macro2::{Literal, Span, TokenStream};
use quote::quote;
use syn::{
    LitStr, Token,
    parse::{Parse, ParseStream},
};

use crate::profile;

/// `asset!("css/app.css")`, or `asset!("data/blob.xyz", "application/octet-stream")`.
struct Input {
    path: LitStr,
    content_type: Option<LitStr>,
}

impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path = input.parse()?;
        let mut content_type = None;

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;

            if !input.is_empty() {
                content_type = Some(input.parse()?);
            }
        }

        Ok(Self { path, content_type })
    }
}

/// Assets already embedded in this compilation unit.
///
/// A proc macro is a dynamic library loaded once per rustc process, and rustc
/// compiles one crate per process, so this lives exactly as long as the crate
/// it describes. Keyed by hashed file name, which already carries the content
/// hash, so the same file referenced from two places is embedded once.
fn embedded() -> &'static Mutex<HashSet<(String, String)>> {
    static EMBEDDED: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    EMBEDDED.get_or_init(Mutex::default)
}

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input: Input = match syn::parse2(input) {
        Ok(parsed) => parsed,
        Err(error) => return error.to_compile_error(),
    };

    let span = input.path.span();

    let Some(root) = env::var_os("CARGO_MANIFEST_DIR") else {
        return fail(
            span,
            "CARGO_MANIFEST_DIR is not set, so there is nothing to resolve this path against",
        );
    };

    let path = PathBuf::from(root).join(input.path.value());
    let content_type = input.content_type.as_ref().map(LitStr::value);

    // Only rust-analyzer and friends land on the fallback, and nothing they
    // produce is served, so the cheaper mode is the right one there.
    let mode = profile::mode().unwrap_or(Mode::Debug);

    let built = match exos_build::build(&path, content_type.as_deref(), mode) {
        Ok(built) => built,
        Err(error) => return fail(span, &describe(&error)),
    };

    let url = built.url();

    // A repeat reference needs the URL and nothing else: the bytes and the
    // registration are already in this crate, and the URL is known here
    // because it is derived from the content.
    if !claim(&built.file) {
        return quote! { #url };
    }

    // Rebuild tracking. Cargo's `rerun-if-changed` belongs to build scripts,
    // but rustc records the files it includes, and cargo reads that. Every
    // file the bundler actually opened is listed, so editing an `@import`ed
    // stylesheet rebuilds and editing an unrelated file does not.
    let sources = built
        .sources
        .iter()
        .map(|source| LitStr::new(&source.display().to_string(), Span::call_site()));

    let name = &built.name;
    let file = &built.file;
    let content_type = &built.content_type;
    let bytes = Literal::byte_string(&built.bytes);

    // Detection reads rustc's command line, so if a future cargo changes how
    // it spells these flags, this fails loudly here instead of quietly
    // shipping a stylesheet nobody minified.
    let checked = profile::debug_assertions().map(|debug| {
        quote! {
            const _: () = assert!(
                #debug == cfg!(debug_assertions),
                "exos: asset! could not tell a debug build from a release one. \
                 Please report this, with your cargo version and profile."
            );
        }
    });

    quote! {
        {
            #( const _: &[u8] = include_bytes!(#sources); )*
            #checked

            ::exos::inventory::submit! {
                ::exos::AssetSetEntry::new(::exos::AssetSet(&[::exos::Asset::new(
                    #name,
                    #file,
                    #content_type,
                    #bytes,
                )]))
            }

            #url
        }
    }
}

/// Whether this call site is the one that embeds the asset.
///
/// Outside an identifiable rustc invocation there is no compilation unit to
/// scope the answer to, and a shared proc-macro server would carry the claim
/// into a later, unrelated expansion. Embedding twice is wasteful; skipping
/// wrongly leaves an asset the router cannot serve.
fn claim(file: &str) -> bool {
    let Some(unit) = profile::unit() else {
        return true;
    };

    embedded()
        .lock()
        .is_ok_and(|mut embedded| embedded.insert((unit, file.to_owned())))
}

/// The whole error chain, since the outer message names the file and the inner
/// one says what was wrong with it.
fn describe(error: &exos_build::Error) -> String {
    let mut message = error.to_string();
    let mut cause = std::error::Error::source(error);

    while let Some(source) = cause {
        message.push_str(&format!(": {source}"));
        cause = source.source();
    }

    message
}

fn fail(span: Span, message: &str) -> TokenStream {
    syn::Error::new(span, message).to_compile_error()
}
