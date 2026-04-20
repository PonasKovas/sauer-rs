macro_rules! make_enum {
    (
        $(#[$($attr:meta)*])*
        [$repr:ty] $name:ident {
            $( $(#[$($variant_attr:meta)*])* $variant:ident = $num:literal, )+
        }
    ) => {
        $(#[$($attr)*])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[repr($repr)]
        pub enum $name {
            $( $(#[$($variant_attr)*])* $variant = $num, )+
        }

        const _: () = {
        #[allow(unused)]
        use crate::internal::{SauerWrite, SauerRead};
        #[allow(unused)]
        use crate::error::EnumConvertResultExt;

        impl SauerWrite for $name {
            fn sauer_write(&self, out: &mut Vec<u8>) {
                (*self as $repr).sauer_write(out);
            }
        }
        impl SauerRead for $name {
            fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
                let repr = <$repr>::sauer_read(reader)?;

                Self::try_from(repr).in_enum(stringify!($name))
            }
        }

        // Enums are represented as strings in lua
        #[cfg(feature = "lua")]
        const _: () = {
            use mlua::{FromLua, IntoLua, Lua, Value};

            impl IntoLua for $name {
                fn into_lua(self, lua: &Lua) -> Result<Value, mlua::Error> {
                    let s = match self {
                        $( $name::$variant => stringify!($variant), )+
                    };

                    Ok(Value::String(lua.create_string(s.as_bytes())?))
                }
            }

            impl FromLua for $name {
                fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
                    let s = String::from_lua(value, lua)?;
                    match s.as_str() {
                        $( stringify!($variant) => Ok($name::$variant), )+
                        _ => Err(mlua::Error::FromLuaConversionError {
                            from: "string",
                            to: stringify!($name).to_owned(),
                            message: Some(format!(
                                "unknown variant '{}' for {}",
                                s, stringify!($name)
                            )),
                        }),
                    }
                }
            }
        };

        };
    };
}

macro_rules! make_struct {
    (
        $(#[$($attr:meta)*])*
        $( [ $TAG:expr ] )?
        $name:ident {
            $( $(#[$($variant_attr:meta)*])*
            $field:ident : $type:ty $(=> $ser:path, $de:path)? ),*
            $(,)?
        }
    ) => {
        $(#[$($attr)*])*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name {
            $(
                $(#[$($variant_attr)*])*
                pub $field : $type,
            )*
        }

        const _: () = {
        #[allow(unused)]
        use crate::internal::{SauerWrite, SauerRead};
        #[allow(unused)]
        use crate::error::ParseResultExt;

        impl SauerWrite for $name {
            fn sauer_write(&self, out: &mut Vec<u8>) {
                $( $TAG.sauer_write(out); )?
                $(
                    make_struct!(@serialize self.$field, out $( => $ser )? );
                )*
            }
        }
        impl SauerRead for $name {
            fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
                $(
                    crate::internal::check_tag(reader, $TAG)?;
                )?

                Ok(Self{
                    $(
                        $field : make_struct!(@deserialize $type, reader $( => $de )? )
                            .in_field(stringify!($field))
                            .in_struct(stringify!($name))?,
                    )*
                })
            }
        }

        #[cfg(feature = "lua")]
        const _: () = {
            use mlua::{FromLua, UserData, UserDataFields};

            impl UserData for $name {
                fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
                    $(
                        fields.add_field_method_get(stringify!($field), |_lua, this| {
                            Ok(this.$field.clone())
                        });
                    )*

                    // avoid unused warning
                    let _ = fields;
                }
            }

            impl FromLua for $name {
                fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
                    #[allow(unused_variables)]
                    let table = mlua::Table::from_lua(value, lua)?;
                    Ok(Self {
                        $(
                            $field: table.get(stringify!($field))?,
                        )*
                    })
                }
            }
        };

        };
    };
    (@serialize $var:expr, $out:expr) => {
        $var.sauer_write($out);
    };
    (@serialize $var:expr, $out:expr => $ser:path) => {
        $ser (&$var, $out);
    };
    (@deserialize $type:ty, $reader:expr) => {
        <$type>::sauer_read($reader)
    };
    (@deserialize $type:ty, $reader:expr => $de:path) => {
        $de($reader)
    };
}

macro_rules! make_packet_enum {
    // Getting the actual struct name, based on whether this is a
    // duplicate variant (like NewMap) and whether its wrapped in Client
    ////////////////////////////////////////////////////////////////////
    (@structname $variant:ident $client_wrapper:ident $DUPLICATE_STRUCT:ident) => {
        $client_wrapper::<$DUPLICATE_STRUCT>
    };
    (@structname $variant:ident $client_wrapper:ident) => {
        $client_wrapper::<$variant>
    };
    (@structname $variant:ident) => {
        $variant
    };

    ////////////////////////////////////////////////////////////////////
    //
    //

    // Helper to determine if a variant is wrapped at compile time
    (@is_wrapped ) => { false };
    (@is_wrapped $wrapper:ident) => { true };

    (@is_reliable $reliable:expr ) => { $reliable };
    (@is_reliable) => { true }; // default


    // Implementing Encode and Decode for the struct
    ////////////////////////////////////////////////////////////////////
    // 1. Unwrapped Base Struct
    (@impl_encode_decode $variant:ident, $ROUTE:path, $( [RELIABLE $reliable:expr] )? ) => {
        impl crate::Encode<$ROUTE> for $variant {
            fn encode(&self, out: &mut Vec<u8>) {
                crate::internal::SauerWrite::sauer_write(self, out);
            }
            fn is_reliable(&self) -> bool {
                make_packet_enum!(@is_reliable $($reliable)? )
            }
        }
        impl crate::Decode<$ROUTE> for $variant {
            fn decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut crate::Session) -> Result<Self, crate::ParseError> {
                if client_number.is_some() {
                    return Err(crate::ParseError::BadClientWrapper);
                }
                crate::internal::SauerReadStateful::sauer_read_stateful(reader, session)
            }
        }
    };

    // 2. N_CLIENT wrapped Base Struct
    (@impl_encode_decode $variant:ident, $ROUTE:path, [ N_CLIENT $client_wrapper:ident ] $( [RELIABLE $reliable:expr] )? ) => {
        impl crate::Encode<$ROUTE> for $client_wrapper<$variant> {
            fn encode(&self, out: &mut Vec<u8>) {
                // Call the private trait, ensuring $variant doesn't need to impl Encode
                crate::internal::SauerWrite::sauer_write(&self.packet, out);
            }
            fn requires_client_wrapper(&self) -> Option<i32> {
                Some(self.client_num)
            }
            fn is_reliable(&self) -> bool {
                make_packet_enum!(@is_reliable $($reliable)? )
            }
        }
        impl crate::Decode<$ROUTE> for $client_wrapper<$variant> {
            fn decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut crate::Session) -> Result<Self, crate::ParseError> {
                match client_number {
                    None => {
                        let mut temp = *reader;
                        let msg_id = <i32 as crate::internal::SauerRead>::sauer_read(&mut temp)?;
                        Err(crate::ParseError::MissingClientWrapper {
                            message_type: crate::internal::msg_id_to_name(msg_id),
                        })
                    }
                    Some(client_num) => Ok(Self {
                        client_num,
                        // Call the private trait here too
                        packet: <$variant as crate::internal::SauerReadStateful>::sauer_read_stateful(reader, session)?,
                    }),
                }
            }
        }
    };

    // 3. N_CLIENT Wrapped Duplicate Struct (e.g. NewMap)
    (@impl_encode_decode $variant:ident, $ROUTE:path, [ N_CLIENT $client_wrapper:ident ]  [ DUPLICATE $DUPLICATE_STRUCT:ident ] $( [RELIABLE $reliable:expr] )? ) => {
        impl crate::Encode<$ROUTE> for $client_wrapper<$DUPLICATE_STRUCT> {
            fn encode(&self, out: &mut Vec<u8>) {
                crate::internal::SauerWrite::sauer_write(&self.packet, out);
            }
            fn requires_client_wrapper(&self) -> Option<i32> {
                Some(self.client_num)
            }
            fn is_reliable(&self) -> bool {
                make_packet_enum!(@is_reliable $($reliable)? )
            }
        }
        impl crate::Decode<$ROUTE> for $client_wrapper<$DUPLICATE_STRUCT> {
            fn decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut crate::Session) -> Result<Self, crate::ParseError> {
                match client_number {
                    None => {
                        let mut temp = *reader;
                        let msg_id = <i32 as crate::internal::SauerRead>::sauer_read(&mut temp)?;
                        Err(crate::ParseError::MissingClientWrapper {
                            message_type: crate::internal::msg_id_to_name(msg_id),
                        })
                    }
                    Some(client_num) => Ok(Self {
                        client_num,
                        packet: <$DUPLICATE_STRUCT as crate::internal::SauerReadStateful>::sauer_read_stateful(reader, session)?,
                    }),
                }
            }
        }
    };

    // 4. Unwrapped Base struct with UPDATE_SESSION
    (@impl_encode_decode $variant:ident, $ROUTE:path, [UPDATE_SESSION $update_closure:expr] $( [RELIABLE $reliable:expr] )? ) => {
        impl crate::Encode<$ROUTE> for $variant {
            fn encode(&self, out: &mut Vec<u8>) {
                crate::internal::SauerWrite::sauer_write(self, out);
            }
            fn is_reliable(&self) -> bool {
                make_packet_enum!(@is_reliable $($reliable)? )
            }
        }
        impl crate::Decode<$ROUTE> for $variant {
            fn decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut crate::Session) -> Result<Self, crate::ParseError> {
                if client_number.is_some() {
                    return Err(crate::ParseError::BadClientWrapper);
                }
                let pkt = crate::internal::SauerReadStateful::sauer_read_stateful(reader, session)?;

                // update session
                ($update_closure)(session, &pkt);

                Ok(pkt)
            }
        }
    };
    ////////////////////////////////////////////////////////////////////
    //
    //

    // API arm
    (
        $(#[$($attr:meta)*])*
        [ $ROUTE:ty ]
        $name:ident {
            $(
                $(#[$($variant_attr:meta)*])*
                [$TAG:path] $( [RELIABLE $reliable:expr] )? $variant:ident $( [N_CLIENT $client_wrapper:ident] $([DUPLICATE $DUPLICATE_STRUCT:ident])? )?
                $( [UPDATE_SESSION $update_closure:expr] )?
            ),+
            $(,)?
        }
    ) => {
        $(#[$($attr)*])*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum $name {
            $(
                $(#[$($variant_attr)*])*
                $variant(
                    make_packet_enum!(@structname $variant $( $client_wrapper $($DUPLICATE_STRUCT)? )? )
                ),
            )+
        }

        const _: () = {
        #[allow(unused)]
        use crate::internal::{SauerWrite, SauerRead};

        $(const _: () = {

            type __Variant = make_packet_enum!(@structname $variant $( $client_wrapper $($DUPLICATE_STRUCT)? )? );

            impl From< __Variant > for $name {
                fn from(v: __Variant ) -> Self {
                    Self::$variant(v)
                }
            }

            make_packet_enum!(@impl_encode_decode $variant, $ROUTE,
                                $( [N_CLIENT $client_wrapper] $([DUPLICATE $DUPLICATE_STRUCT])? )?
                                $( [UPDATE_SESSION $update_closure] )? $( [RELIABLE $reliable] )?
                             );
        };)+

        impl crate::Encode<$ROUTE> for $name {
            fn encode(&self, out: &mut Vec<u8>) {
                match self {
                    $(
                        Self::$variant(v) => crate::Encode::<$ROUTE>::encode(v, out),
                    )+
                }
            }

            fn requires_client_wrapper(&self) -> Option<i32> {
                match self {
                    $(
                        Self::$variant(v) => crate::Encode::<$ROUTE>::requires_client_wrapper(v),
                    )+
                }
            }

            fn is_reliable(&self) -> bool {
                match self {
                    $(
                        Self::$variant(v) => crate::Encode::<$ROUTE>::is_reliable(v),
                    )+
                }
            }
        }
        impl crate::Decode<$ROUTE> for $name {
            fn decode(reader: &mut &[u8], client_number: Option<i32>, session: &mut crate::Session) -> Result<Self, crate::ParseError> {
                let mut temp_reader = *reader;
                let msg_id = <i32 as crate::internal::SauerRead>::sauer_read(&mut temp_reader)?;

                $(
                    if msg_id == $TAG {
                        type __Variant = make_packet_enum!(@structname $variant $( $client_wrapper $($DUPLICATE_STRUCT)? )? );

                        // Automatically disambiguate duplicate tags based on whether we are currently wrapped!
                        let is_wrapped = make_packet_enum!(@is_wrapped $($client_wrapper)?);
                        if is_wrapped == client_number.is_some() {
                            return Ok(Self::$variant(<__Variant as crate::Decode::<$ROUTE>>::decode(reader, client_number, session)?));
                        }
                    }
                )+

                Err(crate::ParseError::UnknownMessageType {
                    route: stringify!($ROUTE),
                    id: msg_id,
                })
            }
        }
        impl $name {
            pub fn msg_type(&self) -> &'static str {
                match self {
                    $(
                        Self::$variant(_) => crate::internal::msg_id_to_name($TAG),
                    )+
                }
            }
        }

        };
    };
}

pub(crate) use make_enum;
pub(crate) use make_packet_enum;
pub(crate) use make_struct;
