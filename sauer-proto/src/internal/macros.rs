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
        pub struct $name {
            $(
                $(#[$($variant_attr)*])*
                $field : $type,
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


    // Implementing Encode and Decode for the struct
    ////////////////////////////////////////////////////////////////////
    // 1. Unwrapped Base Struct
    (@impl_encode_decode $variant:ident, $ROUTE:path, ) => {
        impl crate::Encode<$ROUTE> for $variant {
            fn encode(&self, out: &mut Vec<u8>) {
                crate::internal::SauerWrite::sauer_write(self, out);
            }
        }
        impl crate::Decode<$ROUTE> for $variant {
            fn decode(reader: &mut &[u8], client_number: Option<i32>) -> Result<Self, crate::ParseError> {
                if client_number.is_some() {
                    return Err(crate::ParseError::BadClientWrapper);
                }
                crate::internal::SauerRead::sauer_read(reader)
            }
        }
    };

    // 2. Wrapped Base Struct
    (@impl_encode_decode $variant:ident, $ROUTE:path, $client_wrapper:ident, ) => {
        impl crate::Encode<$ROUTE> for $client_wrapper<$variant> {
            fn encode(&self, out: &mut Vec<u8>) {
                // Call the private trait, ensuring $variant doesn't need to impl Encode
                crate::internal::SauerWrite::sauer_write(&self.packet, out);
            }
            fn requires_client_wrapper(&self) -> Option<i32> {
                Some(self.client_num)
            }
        }
        impl crate::Decode<$ROUTE> for $client_wrapper<$variant> {
            fn decode(reader: &mut &[u8], client_number: Option<i32>) -> Result<Self, crate::ParseError> {
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
                        packet: <$variant as crate::internal::SauerRead>::sauer_read(reader)?,
                    }),
                }
            }
        }
    };

    // 3. Unwrapped Duplicate Struct
    (@impl_encode_decode $variant:ident, $ROUTE:path, , $DUPLICATE_STRUCT:ident) => {
        // Do nothing. The base struct already implements Encode/Decode for this route!
    };

    // 4. Wrapped Duplicate Struct (e.g. NewMap)
    (@impl_encode_decode $variant:ident, $ROUTE:path, $client_wrapper:ident, $DUPLICATE_STRUCT:ident) => {
        impl crate::Encode<$ROUTE> for $client_wrapper<$DUPLICATE_STRUCT> {
            fn encode(&self, out: &mut Vec<u8>) {
                crate::internal::SauerWrite::sauer_write(&self.packet, out);
            }
            fn requires_client_wrapper(&self) -> Option<i32> {
                Some(self.client_num)
            }
        }
        impl crate::Decode<$ROUTE> for $client_wrapper<$DUPLICATE_STRUCT> {
            fn decode(reader: &mut &[u8], client_number: Option<i32>) -> Result<Self, crate::ParseError> {
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
                        packet: <$DUPLICATE_STRUCT as crate::internal::SauerRead>::sauer_read(reader)?,
                    }),
                }
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
                [$TAG:path] $variant:ident $( [N_CLIENT $client_wrapper:ident] $([DUPLICATE $DUPLICATE_STRUCT:ident])? )?
            ),+
            $(,)?
        }
    ) => {
        $(#[$($attr)*])*
        #[derive(Debug, Clone, PartialEq)]
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

            make_packet_enum!(@impl_encode_decode $variant, $ROUTE, $( $client_wrapper , $( $DUPLICATE_STRUCT )? )?  );
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
        }
        impl crate::Decode<$ROUTE> for $name {
            fn decode(reader: &mut &[u8], client_number: Option<i32>) -> Result<Self, crate::ParseError> {
                let mut temp_reader = *reader;
                let msg_id = <i32 as crate::internal::SauerRead>::sauer_read(&mut temp_reader)?;

                $(
                    if msg_id == $TAG {
                        type __Variant = make_packet_enum!(@structname $variant $( $client_wrapper $($DUPLICATE_STRUCT)? )? );

                        // Automatically disambiguate duplicate tags based on whether we are currently wrapped!
                        let is_wrapped = make_packet_enum!(@is_wrapped $($client_wrapper)?);
                        if is_wrapped == client_number.is_some() {
                            return Ok(Self::$variant(<__Variant as crate::Decode::<$ROUTE>>::decode(reader, client_number)?));
                        }
                    }
                )+

                Err(crate::ParseError::UnknownMessageType {
                    route: stringify!($ROUTE),
                    id: msg_id,
                })
            }
        }

        };
    };
}

pub(crate) use make_enum;
pub(crate) use make_packet_enum;
pub(crate) use make_struct;
