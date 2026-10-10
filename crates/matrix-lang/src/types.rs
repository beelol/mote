#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    String,
    Number,
    Integer,
    Boolean,
    Url,
    Asset,
    Date,
    Time,
    DateTime,
    Enum(String),
    Record(String),
    List(Box<Type>),
    Option(Box<Type>),
}

impl Type {
    pub fn is_number(&self) -> bool {
        matches!(self, Self::Number | Self::Integer)
    }

    pub fn is_display(&self) -> bool {
        matches!(
            self,
            Self::String | Self::Number | Self::Integer | Self::Enum(_)
        )
    }

    pub fn accepts(&self, actual: &Self) -> bool {
        self == actual || (*self == Self::Number && *actual == Self::Integer)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DisplayFormat {
    #[default]
    Canonical,
    Fixed {
        decimals: u8,
    },
    Money {
        symbol: String,
        decimals: u8,
    },
}
