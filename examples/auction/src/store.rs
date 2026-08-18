//! The room's data: the lots, the people who can bid on them, and the numbers
//! a guest is known by.
//!
//! The operations on the lots are free functions over a slice rather than
//! methods that reach for global state, which is what lets them be tested
//! against a local `Vec` with no shared state between tests and no reliance on
//! the order they run in.

use std::{collections::HashMap, sync::Mutex};

use exos::Id;

// -----------------------------------------------------------------------------
//                                   THE ROOM
// -----------------------------------------------------------------------------

/// Who is leading a lot.
///
/// The two variants are the two audiences a connection can carry, which is not
/// a coincidence: this is the type the room uses to decide who to tell, and
/// [`toast::tell`](crate::toast::tell) turns one straight into a `send`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Bidder {
    /// Somebody who claimed an account, addressable wherever they are.
    Viewer(u32),
    /// Somebody who has not, addressable as the name their cookie carries.
    ///
    /// This is the whole reason exos hands a resolver the session name as an
    /// `Option` rather than short-circuiting a visit with nobody behind it.
    Guest(Id),
}

/// One raise, by somebody.
#[derive(Clone, Debug)]
pub(crate) struct Bid {
    pub(crate) who: Bidder,
    /// What the lot stood at once this was made, in whole pounds.
    pub(crate) amount: u32,
}

/// One thing being sold.
#[derive(Clone, Debug)]
pub(crate) struct Lot {
    /// The identifier the DOM, the routes and the topic all use.
    pub(crate) id: u32,
    pub(crate) title: String,
    /// What it opened at, before anybody bid.
    pub(crate) reserve: u32,
    /// Every bid on it, oldest first.
    ///
    /// The price and the leader are read off the end of this rather than
    /// stored, which is the whole reason it is a list. A bid can then be taken
    /// back out and both go back to what they were, with nothing having to
    /// remember what that was.
    pub(crate) bids: Vec<Bid>,
    /// Whether the hammer has come down.
    ///
    /// Nothing decides this on a timer. A lot stands until the auctioneer
    /// closes it, which is what makes the moment legible: somebody did it, and
    /// the room can see who.
    pub(crate) closed: bool,
}

impl Lot {
    /// What it stands at.
    #[must_use]
    pub(crate) fn price(&self) -> u32 {
        self.bids.last().map_or(self.reserve, |bid| bid.amount)
    }

    /// Who bid last, and therefore who loses when somebody bids again.
    #[must_use]
    pub(crate) fn leader(&self) -> Option<&Bidder> {
        self.bids.last().map(|bid| &bid.who)
    }
}

/// What a bid changed, so the caller knows who to commiserate with.
#[derive(Clone, Debug)]
pub(crate) struct Accepted {
    pub(crate) title: String,
    pub(crate) price: u32,
    /// Whoever was leading before, if it was not the same person.
    pub(crate) outbid: Option<Bidder>,
}

/// What a bid adds, in pounds. Fixed, so the room needs no number field.
pub(crate) const INCREMENT: u32 = 10;

/// The lots, as application data.
#[derive(Debug, Default)]
pub(crate) struct Lots(Mutex<Vec<Lot>>);

impl Lots {
    /// A sale with something in it, so the example has something to watch.
    #[must_use]
    pub(crate) fn seed() -> Self {
        let seeds = [
            ("A brass ship's clock", 120),
            ("Nineteen feet of shelving", 40),
            ("One unopened compiler manual", 15),
            ("A chair that was in a film", 250),
        ];

        let lots = seeds
            .iter()
            .enumerate()
            .map(|(index, (title, reserve))| Lot {
                id: u32::try_from(index).unwrap_or(0) + 1,
                title: (*title).to_owned(),
                reserve: *reserve,
                bids: Vec::new(),
                closed: false,
            })
            .collect();

        Self(Mutex::new(lots))
    }

    /// A copy of the lots, for rendering.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    #[must_use]
    pub(crate) fn snapshot(&self) -> Vec<Lot> {
        self.0
            .lock()
            .expect("the store lock is never held across a panic")
            .clone()
    }

    /// One lot, which is what a fragment renders from.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`snapshot`](Self::snapshot).
    #[must_use]
    pub(crate) fn one(&self, id: u32) -> Option<Lot> {
        self.0
            .lock()
            .expect("the store lock is never held across a panic")
            .iter()
            .find(|lot| lot.id == id)
            .cloned()
    }

    /// Applies `change` to the lots and hands back whatever it decided.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`snapshot`](Self::snapshot).
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut Vec<Lot>) -> T) -> T {
        change(
            &mut self
                .0
                .lock()
                .expect("the store lock is never held across a panic"),
        )
    }
}

/// Raises `id` by one increment on `who`'s behalf.
///
/// Answers with `None` when there is nothing to raise: an unknown lot, or one
/// the hammer already came down on.
pub(crate) fn bid(lots: &mut [Lot], id: u32, who: &Bidder) -> Option<Accepted> {
    let lot = lots.iter_mut().find(|lot| lot.id == id)?;

    if lot.closed {
        return None;
    }

    // Read before the push, because the push replaces who is leading and this
    // is the whole reason a directed effect is being sent at all.
    let previous = lot.leader().cloned();
    let amount = lot.price() + INCREMENT;

    lot.bids.push(Bid {
        who: who.clone(),
        amount,
    });

    Some(Accepted {
        title: lot.title.clone(),
        price: amount,
        // Bidding against yourself is allowed and is not news.
        outbid: previous.filter(|previous| previous != who),
    })
}

/// Brings the hammer down on `id`, answering with the lot as it finished.
pub(crate) fn close(lots: &mut [Lot], id: u32) -> Option<Lot> {
    let lot = lots.iter_mut().find(|lot| lot.id == id && !lot.closed)?;

    lot.closed = true;

    Some(lot.clone())
}

/// Moves whatever `guest` bid over to `account`, and says which lots changed.
///
/// Signing in is a privilege change, not a new person: somebody who bid as a
/// guest and then claimed an account should not lose what they were winning.
///
/// Open lots only. What a closed one sold for and to whom is a record, and a
/// record that rewrites itself when somebody signs in is not one.
pub(crate) fn claim(lots: &mut [Lot], guest: &Id, account: u32) -> Vec<u32> {
    let guest = Bidder::Guest(guest.clone());

    open(lots)
        .filter_map(|lot| {
            let mut moved = false;

            for bid in lot.bids.iter_mut().filter(|bid| bid.who == guest) {
                bid.who = Bidder::Viewer(account);
                moved = true;
            }

            moved.then_some(lot.id)
        })
        .collect()
}

/// Takes every one of `guest`'s bids back out, and says which lots changed.
///
/// What [`claim`] does for somebody who is going to keep bidding. The
/// auctioneer is not, so carrying their bids into the rostrum would be a way
/// around the rule the bid handler enforces, and leaving them under a name
/// whose cookie has just been rotated away would strand them under somebody
/// nobody can reach.
///
/// Restoring the price and the leader takes no bookkeeping, because both are
/// read off the end of the list: drop the bids and they are what they were.
pub(crate) fn withdraw(lots: &mut [Lot], guest: &Id) -> Vec<u32> {
    let guest = Bidder::Guest(guest.clone());

    open(lots)
        .filter_map(|lot| {
            let before = lot.bids.len();
            lot.bids.retain(|bid| bid.who != guest);

            (lot.bids.len() != before).then_some(lot.id)
        })
        .collect()
}

/// The lots the hammer has not come down on.
fn open(lots: &mut [Lot]) -> impl Iterator<Item = &mut Lot> {
    lots.iter_mut().filter(|lot| !lot.closed)
}

// -----------------------------------------------------------------------------
//                                  THE PEOPLE
// -----------------------------------------------------------------------------

/// What an account is here to do.
///
/// An audience, and an enum rather than a newtype over an id, because not
/// everybody worth addressing is one person.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Role {
    /// Here to bid.
    Bidder,
    /// Here to run the sale.
    Staff,
}

/// One person who can be signed in as.
#[derive(Clone, Debug)]
pub(crate) struct Account {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) role: Role,
}

/// Who exists, and which session name is currently which of them.
///
/// exos knows none of this. It carries a name in a cookie and hands it back;
/// what the name stands for is exactly this table, which in anything real
/// would be a database with an index and an expiry job.
#[derive(Debug)]
pub(crate) struct Accounts {
    people: Vec<Account>,
    live: Mutex<HashMap<Id, u32>>,
}

impl Accounts {
    /// A few people to be, since the example has no sign-up.
    #[must_use]
    pub(crate) fn seed() -> Self {
        let seeds = [
            (1, "Ada", Role::Bidder),
            (2, "Grace", Role::Bidder),
            (3, "The auctioneer", Role::Staff),
        ];

        Self {
            people: seeds
                .iter()
                .map(|(id, name, role)| Account {
                    id: *id,
                    name: (*name).to_owned(),
                    role: *role,
                })
                .collect(),
            live: Mutex::new(HashMap::new()),
        }
    }

    /// Everybody it is possible to be.
    #[must_use]
    pub(crate) fn everybody(&self) -> &[Account] {
        &self.people
    }

    /// Who `name` stands for, if anybody.
    ///
    /// Async because resolving a session name is a database call in anything
    /// real, and that is why the resolver exos takes awaits.
    pub(crate) async fn of(&self, name: &Id) -> Option<Account> {
        let id = *self.lock().get(name)?;

        self.account(id)
    }

    /// One person by id, which is what a fragment has rather than a name.
    #[must_use]
    pub(crate) fn account(&self, id: u32) -> Option<Account> {
        self.people.iter().find(|account| account.id == id).cloned()
    }

    /// Binds `name` to an account, which is the application's half of signing
    /// in. exos's half is the rotation that produced the name.
    pub(crate) async fn claim(&self, name: &Id, account: u32) {
        self.lock().insert(name.clone(), account);
    }

    /// Drops whatever was kept under `name`.
    ///
    /// This has to happen, because a name the browser stopped sending is not a
    /// name nobody else has.
    pub(crate) async fn forget(&self, name: &Id) {
        self.lock().remove(name);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Id, u32>> {
        self.live
            .lock()
            .expect("the store lock is never held across a panic")
    }
}

/// The numbers guests are known by, so the room can talk about somebody it has
/// no name for.
#[derive(Debug, Default)]
pub(crate) struct Guests(Mutex<HashMap<Id, u32>>);

impl Guests {
    /// The number `name` already has, or the next one.
    ///
    /// Called while serving a page, never while rendering a fragment: a
    /// fragment that assigned something would render differently the first
    /// time, and the same topic has to mean the same HTML.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    pub(crate) fn issue(&self, name: &Id) -> u32 {
        let mut numbers = self
            .0
            .lock()
            .expect("the store lock is never held across a panic");

        let next = u32::try_from(numbers.len()).unwrap_or(0) + 1;

        *numbers.entry(name.clone()).or_insert(next)
    }

    /// The number `name` was given, if it has been in the room before.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`issue`](Self::issue).
    #[must_use]
    pub(crate) fn number(&self, name: &Id) -> Option<u32> {
        self.0
            .lock()
            .expect("the store lock is never held across a panic")
            .get(name)
            .copied()
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn lots() -> Vec<Lot> {
        vec![
            Lot {
                id: 1,
                title: String::from("A clock"),
                reserve: 100,
                bids: Vec::new(),
                closed: false,
            },
            Lot {
                id: 2,
                title: String::from("A chair"),
                reserve: 50,
                bids: Vec::new(),
                closed: true,
            },
        ]
    }

    fn ada() -> Bidder {
        Bidder::Viewer(1)
    }

    fn grace() -> Bidder {
        Bidder::Viewer(2)
    }

    #[test]
    fn a_bid_raises_the_price_and_takes_the_lead() {
        let mut lots = lots();

        let accepted = bid(&mut lots, 1, &ada()).expect("the lot is open");

        assert_eq!(accepted.price, 100 + INCREMENT);
        assert_eq!(lots[0].price(), 100 + INCREMENT);
        assert_eq!(lots[0].leader(), Some(&ada()));
    }

    /// The one fact the whole example is built on: a bid names somebody who
    /// just lost, and they are not the person who made it.
    #[test]
    fn a_bid_says_who_it_pushed_out() {
        let mut lots = lots();

        assert!(
            bid(&mut lots, 1, &ada())
                .expect("the lot is open")
                .outbid
                .is_none(),
            "the first bid pushes nobody out"
        );

        let accepted = bid(&mut lots, 1, &grace()).expect("the lot is open");

        assert_eq!(accepted.outbid, Some(ada()));
    }

    /// Otherwise a keen bidder would be told they had outbid themselves, which
    /// is true and is not news.
    #[test]
    fn bidding_against_yourself_pushes_nobody_out() {
        let mut lots = lots();

        drop(bid(&mut lots, 1, &ada()));
        let accepted = bid(&mut lots, 1, &ada()).expect("the lot is open");

        assert!(accepted.outbid.is_none());
        assert_eq!(lots[0].price(), 100 + INCREMENT * 2);
    }

    #[test]
    fn a_closed_lot_takes_no_more_bids() {
        let mut lots = lots();

        assert!(bid(&mut lots, 2, &ada()).is_none());
        assert!(
            bid(&mut lots, 99, &ada()).is_none(),
            "nor does a missing one"
        );
        assert_eq!(lots[1].price(), 50);
    }

    #[test]
    fn closing_happens_once() {
        let mut lots = lots();

        assert_eq!(close(&mut lots, 1).expect("it was open").id, 1);
        assert!(lots[0].closed);
        assert!(close(&mut lots, 1).is_none(), "and not twice");
    }

    /// Somebody who bid as a guest and then claimed an account keeps what they
    /// were winning, because signing in is a privilege change and not a new
    /// person arriving.
    #[test]
    fn claiming_an_account_carries_a_guest_s_lots_over() {
        let mut lots = lots();
        let guest = Id::random();

        drop(bid(&mut lots, 1, &Bidder::Guest(guest.clone())));

        assert_eq!(claim(&mut lots, &guest, 7), vec![1]);
        assert_eq!(lots[0].leader(), Some(&Bidder::Viewer(7)));

        assert!(
            claim(&mut lots, &guest, 7).is_empty(),
            "and there is nothing left to carry"
        );
    }

    /// What a lot sold for and to whom is a record. Signing in afterwards must
    /// not rewrite it, or the sale room's books would depend on what the buyer
    /// did next.
    #[test]
    fn claiming_an_account_leaves_a_finished_sale_alone() {
        let mut lots = lots();
        let guest = Id::random();

        drop(bid(&mut lots, 1, &Bidder::Guest(guest.clone())));
        drop(close(&mut lots, 1));

        assert!(claim(&mut lots, &guest, 7).is_empty());
        assert_eq!(lots[0].leader(), Some(&Bidder::Guest(guest)));
    }

    /// The auctioneer does not bid, so taking the rostrum gives up whatever
    /// this browser bid on the way in. Carrying it over instead would be a way
    /// around the refusal in the bid handler.
    #[test]
    fn withdrawing_puts_a_lot_back_where_it_stood() {
        let mut lots = lots();
        let guest = Id::random();
        let guest_bid = Bidder::Guest(guest.clone());

        drop(bid(&mut lots, 1, &guest_bid)); // 110
        drop(bid(&mut lots, 1, &ada())); // 120
        drop(bid(&mut lots, 1, &guest_bid)); // 130

        assert_eq!(withdraw(&mut lots, &guest), vec![1]);

        // Both come back on their own, because both are read off the end of
        // the list rather than kept beside it.
        assert_eq!(lots[0].leader(), Some(&ada()));

        // Ada's own bid, not what it would be if the room had never heard the
        // withdrawn ones. She said 120 and is held to it, which is why a bid
        // carries its amount rather than the price being counted back up from
        // the reserve.
        assert_eq!(lots[0].price(), 120);

        assert!(
            withdraw(&mut lots, &guest).is_empty(),
            "and there is nothing left to withdraw"
        );
    }

    #[test]
    fn withdrawing_every_bid_puts_a_lot_back_to_its_reserve() {
        let mut lots = lots();
        let guest = Id::random();

        drop(bid(&mut lots, 1, &Bidder::Guest(guest.clone())));
        drop(withdraw(&mut lots, &guest));

        assert_eq!(lots[0].price(), 100);
        assert_eq!(lots[0].leader(), None);
    }

    /// The same rule as claiming, for the same reason.
    #[test]
    fn withdrawing_leaves_a_finished_sale_alone() {
        let mut lots = lots();
        let guest = Id::random();

        drop(bid(&mut lots, 1, &Bidder::Guest(guest.clone())));
        drop(close(&mut lots, 1));

        assert!(withdraw(&mut lots, &guest).is_empty());
        assert_eq!(lots[0].leader(), Some(&Bidder::Guest(guest)));
    }

    #[test]
    fn a_guest_keeps_the_number_it_was_given() {
        let guests = Guests::default();
        let first = Id::random();
        let second = Id::random();

        assert_eq!(guests.issue(&first), 1);
        assert_eq!(guests.issue(&first), 1, "asking again is the same answer");
        assert_eq!(guests.issue(&second), 2);

        assert_eq!(guests.number(&first), Some(1));
        assert_eq!(guests.number(&Id::random()), None);
    }

    #[tokio::test]
    async fn a_name_nothing_is_kept_under_stands_for_nobody() {
        let accounts = Accounts::seed();
        let name = Id::random();

        assert!(accounts.of(&name).await.is_none());

        accounts.claim(&name, 1).await;
        assert_eq!(accounts.of(&name).await.expect("Ada").name, "Ada");

        accounts.forget(&name).await;
        assert!(accounts.of(&name).await.is_none());
    }
}
