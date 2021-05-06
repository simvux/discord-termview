use std::fmt;

const HEIGHT_LIMIT: usize = 1000;

#[derive(Debug)]
pub enum Command {
    New { height: usize, private: bool },
    Remove,
    Run(String),
}

pub fn parse(raw: &str) -> Result<Command, Error> {
    if raw.starts_with('`') {
        return parse_run(raw);
    }

    let mut iter = raw.split(' ');

    let header = iter.next().ok_or(Error::NoAction)?;

    match header {
        "new" => parse_new(iter),
        "remove" => Ok(parse_remove(iter)),
        "run" => Ok(Command::Run(iter.collect::<String>())),
        faulty => Err(Error::UnrecognizedCommand(faulty.to_string())),
    }
}

fn parse_run(raw: &str) -> Result<Command, Error> {
    let ends_at = raw[1..].find('`').ok_or(Error::MissingEndToCodeBlock)?;
    let code = &raw[1..=ends_at];
    Ok(Command::Run(code.to_string()))
}

fn parse_remove<'a>(_iter: impl Iterator<Item = &'a str>) -> Command {
    Command::Remove
}

fn parse_new<'a>(iter: impl Iterator<Item = &'a str>) -> Result<Command, Error> {
    let mut height = 20;
    let mut private = true;

    for word in iter {
        if word.starts_with("height") {
            height = word
                .get(7..)
                .ok_or(Error::MissingArgument("int after 'height'"))
                .and_then(|s| s.parse().map_err(|_| Error::InvalidNumber))?;
        }

        if word == "private" {
            private = true;
        }
    }

    if height > HEIGHT_LIMIT {
        return Err(Error::HeightToLarge(height));
    }

    Ok(Command::New { height, private })
}

#[derive(Debug)]
pub enum Error {
    NoAction,
    UnrecognizedCommand(String),
    MissingArgument(&'static str),
    HeightToLarge(usize),
    InvalidNumber,
    InvalidBool,
    MissingEndToCodeBlock,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NoAction => f.write_str("no command was supplied"),
            Error::UnrecognizedCommand(faulty) => write!(f, "{} is not a valid command", faulty),
            Error::MissingArgument(missing) => write!(f, "missing required argument '{}'", missing),
            Error::InvalidNumber => f.write_str("not a valid number"),
            Error::InvalidBool => f.write_str("not a valid boolean"),
            Error::HeightToLarge(height) => write!(
                f,
                "height limit is {} but you tried to set it to {}",
                HEIGHT_LIMIT, height
            ),
            Error::MissingEndToCodeBlock => f.write_str("missing end to code block"),
        }
    }
}
