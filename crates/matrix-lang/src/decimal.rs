use std::cmp::Ordering;

pub(crate) struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    order: Exponent,
}

impl Decimal {
    pub(crate) fn from_source(source: &str) -> Self {
        let negative = source.starts_with('-');
        let unsigned = source.trim_start_matches('-');
        let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
        let integer_digits = mantissa.find('.').unwrap_or(mantissa.len());
        let digits: Vec<u8> = mantissa
            .bytes()
            .filter(|b| *b != b'.')
            .map(|b| b - b'0')
            .collect();
        let Some(first) = digits.iter().position(|d| *d != 0) else {
            return Self {
                negative: false,
                digits: Vec::new(),
                order: Exponent::parse("0"),
            };
        };
        let end = digits.iter().rposition(|d| *d != 0).unwrap() + 1;
        let order = Exponent::parse(exponent).add(Exponent::parse(
            &(integer_digits as isize - first as isize).to_string(),
        ));
        Self {
            negative,
            digits: digits[first..end].to_vec(),
            order,
        }
    }

    fn sign(&self) -> i8 {
        if self.digits.is_empty() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub(crate) fn is_positive(&self) -> bool {
        self.sign() > 0
    }

    pub(crate) fn is_nonnegative_integer(&self) -> bool {
        self.sign() == 0
            || (!self.negative
                && self
                    .order
                    .cmp(&Exponent::parse(&self.digits.len().to_string()))
                    != Ordering::Less)
    }

    pub(crate) fn cmp(&self, other: &Self) -> Ordering {
        let sign = self.sign().cmp(&other.sign());
        if sign != Ordering::Equal || self.sign() == 0 {
            return sign;
        }
        let magnitude = self
            .order
            .cmp(&other.order)
            .then_with(|| self.digits.cmp(&other.digits));
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

struct Exponent {
    negative: bool,
    // Least significant digits first keep exponent adjustment bounded by source length.
    digits: Vec<u8>,
}

impl Exponent {
    fn parse(source: &str) -> Self {
        let mut value = Self {
            negative: source.starts_with('-'),
            digits: source
                .trim_start_matches(['-', '+'])
                .bytes()
                .rev()
                .map(|b| b - b'0')
                .collect(),
        };
        value.normalize();
        value
    }

    fn normalize(&mut self) {
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.negative = false;
        }
    }

    fn magnitude_cmp(&self, other: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&other.digits.len())
            .then_with(|| self.digits.iter().rev().cmp(other.digits.iter().rev()))
    }

    fn cmp(&self, other: &Self) -> Ordering {
        if self.negative != other.negative {
            return other.negative.cmp(&self.negative);
        }
        let magnitude = self.magnitude_cmp(other);
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    fn add(mut self, mut other: Self) -> Self {
        if self.negative == other.negative {
            self.digits
                .resize(self.digits.len().max(other.digits.len()), 0);
            let mut carry = 0;
            for (i, digit) in self.digits.iter_mut().enumerate() {
                let sum = *digit + other.digits.get(i).copied().unwrap_or(0) + carry;
                *digit = sum % 10;
                carry = sum / 10;
            }
            if carry != 0 {
                self.digits.push(carry);
            }
        } else {
            if self.magnitude_cmp(&other) == Ordering::Less {
                std::mem::swap(&mut self, &mut other);
            }
            let mut borrow = 0;
            for (i, digit) in self.digits.iter_mut().enumerate() {
                let difference =
                    *digit as i8 - other.digits.get(i).copied().unwrap_or(0) as i8 - borrow;
                borrow = i8::from(difference < 0);
                *digit = (difference + 10 * borrow) as u8;
            }
        }
        self.normalize();
        self
    }
}
