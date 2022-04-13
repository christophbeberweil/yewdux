use std::rc::Rc;

use anymap::AnyMap;
use slab::Slab;

use crate::{
    mrc::Mrc,
    store::Store,
    subscriber::{Callable, SubscriberId},
};

pub(crate) struct Context<S> {
    pub(crate) store: Rc<S>,
    pub(crate) subscribers: Slab<Box<dyn Callable<S>>>,
}

impl<S: Store> Context<S> {
    /// Apply a function to state, returning if it has changed or not.
    pub(crate) fn reduce(&mut self, f: impl FnOnce(&mut S)) -> bool {
        let previous = Rc::clone(&self.store);
        let store = Rc::make_mut(&mut self.store);

        f(store);

        let changed = previous.as_ref() != store;

        if changed {
            store.changed();
        }

        changed
    }

    pub(crate) fn subscribe(&mut self, on_change: impl Callable<S>) -> SubscriberId<S> {
        // Notify subscriber with inital state.
        on_change.call(Rc::clone(&self.store));

        let key = self.subscribers.insert(Box::new(on_change));

        SubscriberId {
            key,
            _store_type: Default::default(),
        }
    }

    pub(crate) fn unsubscribe(&mut self, id: usize) {
        self.subscribers.remove(id);
    }

    pub(crate) fn notify_subscribers(&self) {
        for (_, subscriber) in &self.subscribers {
            subscriber.call(Rc::clone(&self.store));
        }
    }
}

pub(crate) fn get_or_init<S: Store>() -> Mrc<Context<S>> {
    thread_local! {
        /// Stores all shared state.
        static CONTEXTS: Mrc<AnyMap> = Mrc::new(AnyMap::new());
    }

    CONTEXTS
        .try_with(|context| context.clone())
        .expect("CONTEXTS thread local key init failed")
        .with_mut(|contexts| {
            contexts
                .entry::<Mrc<Context<S>>>()
                .or_insert_with(|| {
                    Mrc::new(Context {
                        store: Rc::new(S::new()),
                        subscribers: Default::default(),
                    })
                })
                .clone()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq)]
    struct TestState(u32);
    impl Store for TestState {
        fn new() -> Self {
            Self(0)
        }

        fn changed(&mut self) {
            self.0 += 1;
        }
    }

    #[test]
    fn store_changed_is_called() {
        let mut context = get_or_init::<TestState>();

        context.with_mut(|context| context.reduce(|state| state.0 += 1));

        assert!(context.borrow().store.0 == 2);
    }

    #[test]
    fn store_changed_is_not_called_when_state_is_same() {
        let mut context = get_or_init::<TestState>();

        context.with_mut(|context| context.reduce(|_| {}));

        assert!(context.borrow().store.0 == 0);
    }
}
