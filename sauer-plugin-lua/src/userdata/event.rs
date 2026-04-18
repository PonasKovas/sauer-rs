use mlua::{UserData, UserDataMethods};

pub struct EventUD {
    pub cancelled: bool,
}

impl EventUD {
    pub fn new() -> Self {
        Self { cancelled: false }
    }
}

impl UserData for EventUD {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("cancel", |_lua, this, _: ()| {
            this.cancelled = true;

            Ok(())
        });
    }
}
