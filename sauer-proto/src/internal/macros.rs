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

                Ok(Self{
                    $(
                        $field : make_struct!(@deserialize $type, reader $( => $de )? ),
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
        <$type>::sauer_read($reader)?
    };
    (@deserialize $type:ty, $reader:expr => $de:path) => {
        $de($reader)?
    };
}

pub(crate) use make_enum;
pub(crate) use make_struct;
