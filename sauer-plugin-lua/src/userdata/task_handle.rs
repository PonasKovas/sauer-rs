use mlua::{UserData, UserDataMethods};
use tokio::task::JoinHandle;

pub struct TaskHandle(pub JoinHandle<()>);

impl UserData for TaskHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("cancel", |_, this, ()| {
            this.0.abort();
            Ok(())
        });
    }
}
