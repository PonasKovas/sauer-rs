use mlua::{UserData, UserDataFields};
use sauer_proto::data::{ArmourType, Gun, PlayerState};

/// Mutable wrapper around `PlayerState` exposed to Lua in `on_spawn` callbacks.
/// Fields can be read and written; enum fields use their string names (e.g. `"Pistol"`, `"Blue"`).
/// Ammo fields are flattened as `ammo_<weapon>`.
pub struct SpawnStateUD(pub PlayerState);

impl UserData for SpawnStateUD {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        /// Current health points. (read/write)
        fields.add_field_method_get("health", |_, this| Ok(this.0.health));
        fields.add_field_method_set("health", |_, this, val: i32| {
            this.0.health = val;
            Ok(())
        });

        /// Maximum health points. (read/write)
        fields.add_field_method_get("max_health", |_, this| Ok(this.0.max_health));
        fields.add_field_method_set("max_health", |_, this, val: i32| {
            this.0.max_health = val;
            Ok(())
        });

        /// Current armour points. (read/write)
        fields.add_field_method_get("armour", |_, this| Ok(this.0.armour));
        fields.add_field_method_set("armour", |_, this, val: i32| {
            this.0.armour = val;
            Ok(())
        });

        /// Armour type: `"None"`, `"Green"`, `"Yellow"`, or `"Blue"`. (read/write)
        fields.add_field_method_get("armour_type", |_, this| Ok(this.0.armour_type));
        fields.add_field_method_set("armour_type", |_, this, val: ArmourType| {
            this.0.armour_type = val;
            Ok(())
        });

        /// Currently selected gun (e.g. `"Pistol"`, `"Shotgun"`). (read/write)
        fields.add_field_method_get("gun_select", |_, this| Ok(this.0.gun_select));
        fields.add_field_method_set("gun_select", |_, this, val: Gun| {
            this.0.gun_select = val;
            Ok(())
        });

        /// Shotgun ammo. (read/write)
        fields.add_field_method_get("ammo_shotgun", |_, this| Ok(this.0.ammo.shotgun));
        fields.add_field_method_set("ammo_shotgun", |_, this, val: i32| {
            this.0.ammo.shotgun = val;
            Ok(())
        });

        /// Chaingun ammo. (read/write)
        fields.add_field_method_get("ammo_chaingun", |_, this| Ok(this.0.ammo.chaingun));
        fields.add_field_method_set("ammo_chaingun", |_, this, val: i32| {
            this.0.ammo.chaingun = val;
            Ok(())
        });

        /// Rocket launcher ammo. (read/write)
        fields.add_field_method_get("ammo_rocket_launcher", |_, this| {
            Ok(this.0.ammo.rocket_launcher)
        });
        fields.add_field_method_set("ammo_rocket_launcher", |_, this, val: i32| {
            this.0.ammo.rocket_launcher = val;
            Ok(())
        });

        /// Rifle ammo. (read/write)
        fields.add_field_method_get("ammo_rifle", |_, this| Ok(this.0.ammo.rifle));
        fields.add_field_method_set("ammo_rifle", |_, this, val: i32| {
            this.0.ammo.rifle = val;
            Ok(())
        });

        /// Grenade launcher ammo. (read/write)
        fields.add_field_method_get("ammo_grenade_launcher", |_, this| {
            Ok(this.0.ammo.grenade_launcher)
        });
        fields.add_field_method_set("ammo_grenade_launcher", |_, this, val: i32| {
            this.0.ammo.grenade_launcher = val;
            Ok(())
        });

        /// Pistol ammo. (read/write)
        fields.add_field_method_get("ammo_pistol", |_, this| Ok(this.0.ammo.pistol));
        fields.add_field_method_set("ammo_pistol", |_, this, val: i32| {
            this.0.ammo.pistol = val;
            Ok(())
        });
    }
}
