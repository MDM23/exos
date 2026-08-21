# Expressions

`Js<T>` is a client-side expression of type `T`.

| on | methods |
| --- | --- |
| `Js<bool>` | `and`, `not`, `or` |
| numbers | `eq`, `ge`, `gt`, `le`, `lt`, `minus`, `ne`, `plus` |
| `Js<String>` | `contains`, `eq`, `is_empty`, `len`, `ne`, `trim` |
| `Js<Vec<T>>` | `any`, `contains`, `is_empty`, `len` |

`!` is overloadable, so `!gone.get()` works. `==` and `&&` are not, because
`PartialEq::eq` must return `bool`, hence `.eq()` and `.and()`. That is the one
place this API is uglier than the language it mimics, and there is no way
around it.

## The escape hatch

```rust
let coarse = Js::<bool>::raw("matchMedia('(hover: none)').matches");

view! {
    <div {show(coarse)}>"Tap to reveal"</div>
}
```

The type parameter is an assertion the compiler cannot check: you are promising
the expression yields a `bool`. That is the entire cost of the escape hatch,
and it is the only unchecked thing in the API.
