use cube_string::CubeString;
use mlua::{UserData, UserDataFields};

/// The damage about to be dealt, in `on_damage`. Change `amount` to change it.
pub struct DamageUD {
    pub amount: i32,
}

impl UserData for DamageUD {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        /// Damage points. (read/write)
        fields.add_field_method_get("amount", |_, this| Ok(this.amount));
        fields.add_field_method_set("amount", |_, this, val: i32| {
            this.amount = val;
            Ok(())
        });
    }
}

/// A chat message about to be relayed, in `on_chat`. Change `text` to change it.
pub struct ChatUD {
    pub text: CubeString,
}

impl UserData for ChatUD {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        /// The message. (read/write)
        fields.add_field_method_get("text", |_, this| Ok(this.text.clone()));
        fields.add_field_method_set("text", |_, this, val: CubeString| {
            this.text = val;
            Ok(())
        });
    }
}
