use std::num::ParseIntError;
use std::error::Error;
use std::fmt;
use std::string::FromUtf8Error;

pub fn decode_url_str(url: &str) -> Result<String, DecodeError> {
    let mut decoder = Decoder::new();
    for c in url.chars() {
        decoder.process_char(c)?;
    }
    decoder.finalize()
}

struct Decoder {
    output_buffer: String,
    parse_buffer: String,
    state: DecoderState,
}

impl Decoder {
    fn new() -> Self {
        Self {
            output_buffer: String::new(),
            parse_buffer: String::new(),
            state: DecoderState::Reading,
        }
    }

    fn process_char(&mut self, c: char) -> Result<(), DecodeError> {
        match self.state {
            DecoderState::Reading => {
                match c {
                    '%' => self.state = DecoderState::Parsing,
                    '_' => self.output_buffer.push(' '),
                    _ => self.output_buffer.push(c),
                };
            }
            DecoderState::Parsing => {
                self.parse_buffer.push(c);
                if self.parse_buffer.len() % 2 == 0 {
                    self.state = DecoderState::ParseReady;
                }
            }
            DecoderState::ParseReady => {
                if c == '%' {
                    self.state = DecoderState::Parsing;
                } else {
                    let parsed = Self::hex_string_to_unicode(&self.parse_buffer)?;
                    self.output_buffer += &parsed;
                    self.parse_buffer.clear();
                    self.output_buffer.push(c);
                    self.state = DecoderState::Reading;
                }
            }
        }
        Ok(())
    }

    fn hex_string_to_unicode(hex_code: &str) -> Result<String, DecodeError> {
        const HEX_CHARS_PER_BYTE: usize = 2;

        if hex_code.len() % HEX_CHARS_PER_BYTE  != 0 {
            return Err(DecodeError::OddLengthHexString);
        }

        let mut bytes = Vec::with_capacity(hex_code.len() / HEX_CHARS_PER_BYTE);

        for i in (0..hex_code.len()).step_by(HEX_CHARS_PER_BYTE) {
            let slice = &hex_code[i..(i+HEX_CHARS_PER_BYTE)];
            let byte = u8::from_str_radix(slice, 16)?;
            bytes.push(byte);
        }

        let unicode_string = String::from_utf8(bytes)?;
        Ok(unicode_string)
    }

    fn finalize(mut self) -> Result<String, DecodeError> {
        match self.state {
            DecoderState::Reading => Ok(self.output_buffer),
            DecoderState::Parsing => Err(DecodeError::IncompleteParse),
            DecoderState::ParseReady => {
                let parsed = Self::hex_string_to_unicode(&self.parse_buffer)?;
                self.output_buffer += &parsed;
                Ok(self.output_buffer)
            }
        }
    }
}

enum DecoderState {
    Reading,
    Parsing,
    ParseReady,
}

#[derive(Debug)]
pub enum DecodeError {
    OddLengthHexString,
    HexNotValidByte,
    ByteVecNotUtf8,
    IncompleteParse
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::OddLengthHexString => format!("Hex String has odd number of characters"),
            Self::HexNotValidByte => format!("Failed to convert hex code to u8 value"),
            Self::ByteVecNotUtf8 => format!("Bytes from hex string is not valid utf8"),
            Self::IncompleteParse => format!("String ended on incomplete hex code"),
        };
        write!(f, "{}", msg)
    }
}

impl From<ParseIntError> for DecodeError {
    fn from(_: ParseIntError) -> Self {
        DecodeError::HexNotValidByte
    }
}

impl From<FromUtf8Error> for DecodeError {
    fn from(_: FromUtf8Error) -> Self {
        DecodeError::ByteVecNotUtf8
    }
}

impl Error for DecodeError {
    //TODO
}
