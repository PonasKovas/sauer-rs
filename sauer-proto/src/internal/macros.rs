macro_rules! make_enum {
    (
        $(#[$($attr:meta)*])*
        [$repr:ty] $name:ident {
            $( $(#[$($variant_attr:meta)*])* $variant:ident = $num:literal, )+
        }
    ) => {
        $(#[$($attr)*])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TryFromPrimitive)]
        #[repr($repr)]
        pub enum $name {
            $( $(#[$($variant_attr)*])* $variant = $num, )+
        }

        impl SauerWrite for $name {
            fn sauer_write(&self, out: &mut Vec<u8>) {
                (*self as $repr).sauer_write(out);
            }
        }
        impl SauerRead for $name {
            fn sauer_read(reader: &mut &[u8]) -> Result<Self, crate::ParseError> {
                let repr = <$repr>::sauer_read(reader)?;

                use crate::error::EnumConvertResultExt;
                Self::try_from(repr).in_enum(stringify!($name))
            }
        }
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
        pub struct $name {
            $(
                $(#[$($variant_attr)*])*
                $field : $type,
            )*
        }

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

                #[allow(unused)]
                use crate::error::ParseResultExt;

                Ok(Self{
                    $(
                        $field : make_struct!(@deserialize $type, reader $( => $de )? )
                            .in_field(stringify!($field))
                            .in_struct(stringify!($name))?,
                    )*
                })
            }
        }
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
    (
        $(#[$($attr:meta)*])*
        [ $ROUTE:ty ]
        $name:ident {
            $(
                $(#[$($variant_attr:meta)*])*
                [$TAG:path] $variant:ident
            ),+
            $(,)?
        }
    ) => {
        $(#[$($attr)*])*
        #[derive(Debug, Clone, PartialEq)]
        pub enum $name {
            $(
                $(#[$($variant_attr)*])* $variant($variant),
            )+
        }

        $(
            impl From<$variant> for $name {
                fn from(v: $variant) -> Self {
                    Self::$variant(v)
                }
            }
            impl Encode<$ROUTE> for $variant {}
            impl Decode<$ROUTE> for $variant {
                fn decode(reader: &mut &[u8], client_number: Option<i32>) -> Result<Self, ParseError> {
                    if client_number.is_some() {
                        return Err(ParseError::BadClientWrapper);
                    }

                    Self::sauer_read(reader)
                }
            }
        )+

        impl SauerWrite for $name {
            fn sauer_write(&self, out: &mut Vec<u8>) {
                match self {
                    $(
                        Self::$variant(v) => v.sauer_write(out),
                    )+
                }
            }
        }
        impl SauerRead for $name {
            fn sauer_read(reader: &mut &[u8]) -> Result<Self, ParseError> {
                let msg_type = i32::sauer_read(reader)?;

                Ok(match msg_type {
                    $(
                        $TAG => Self::$variant($variant::sauer_read(reader)?),
                    )+
                    other => return Err(ParseError::UnknownMessageType {
                        route: stringify!($ROUTE),
                        id: other,
                    }),
                })
            }
        }
        impl Encode<$ROUTE> for $name {}
        impl Decode<$ROUTE> for $name {
            fn decode(reader: &mut &[u8], client_number: Option<i32>) -> Result<Self, ParseError> {
                if client_number.is_some() {
                    return Err(ParseError::BadClientWrapper);
                }

                Self::sauer_read(reader)
            }
        }
    };
}

pub(crate) use make_enum;
pub(crate) use make_packet_enum;
pub(crate) use make_struct;
