//! The `when` expression language.
//!
//! Deliberately tiny: four variables, two comparisons, two connectives. That is
//! enough for "only deploy from master" and "only publish on a tag", which is
//! what stage conditions are actually for. Anything that wants arithmetic, or
//! to call out to something, belongs in a shell step where it is visible in the
//! log rather than hidden in a condition.
//!
//! ```text
//! expr := and ( "||" and )*
//! and  := cmp ( "&&" cmp )*
//! cmp  := variable ( "==" | "!=" ) string
//! ```
//!
//! `&&` binds tighter than `||`, as everywhere else. There are no parentheses -
//! adding them would mean the language could express things the evaluator has
//! to explain, and every condition that has needed one so far was better off as
//! two stages.

use std::fmt;

/// What a condition may look at.
///
/// A ref is either a branch or a tag, never both: on a tag build `branch` is
/// empty, and on a branch build `tag` is. That is what makes `tag != ''` the
/// natural way to write "only on a tag".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variable {
    /// The branch a push or PR targets; empty on a tag build.
    Branch,
    /// The tag a push targets; empty on a branch build.
    Tag,
    /// The trigger kind: `push`, `pull_request`, or `manual`.
    Event,
    /// The commit being built.
    Sha,
}

impl Variable {
    /// Every variable a condition can name, in the order `known()` reports them.
    pub const ALL: [Self; 4] = [Self::Branch, Self::Tag, Self::Event, Self::Sha];

    /// The identifier a condition source uses for this variable.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Tag => "tag",
            Self::Event => "event",
            Self::Sha => "sha",
        }
    }

    /// Looks up a variable by the identifier a condition source used, if any variable is named that.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == raw)
    }

    fn known() -> String {
        Self::ALL
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

impl CompareOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }
}

/// Everything a condition can see about the run being planned.
#[derive(Clone, Debug, Default)]
pub struct EvalContext {
    /// The branch being built; empty on a tag build.
    pub branch: String,
    /// The tag being built; empty on a branch build.
    pub tag: String,
    /// The trigger kind: `push`, `pull_request`, or `manual`.
    pub event: String,
    /// The commit being built.
    pub sha: String,
}

impl EvalContext {
    /// Builds the context from a run's ref, splitting it into the branch or the
    /// tag but never both.
    ///
    /// `git_ref` is accepted in full (`refs/heads/master`) or bare (`master`);
    /// a bare ref is read as a branch, which is what a manual trigger sends.
    pub fn new(event: &str, git_ref: &str, sha: &str) -> Self {
        let (branch, tag) = match git_ref.strip_prefix("refs/tags/") {
            Some(tag) => (String::new(), tag.to_string()),
            None => (
                git_ref
                    .strip_prefix("refs/heads/")
                    .unwrap_or(git_ref)
                    .to_string(),
                String::new(),
            ),
        };

        Self {
            branch,
            tag,
            event: event.to_string(),
            sha: sha.to_string(),
        }
    }

    fn get(&self, variable: Variable) -> &str {
        match variable {
            Variable::Branch => &self.branch,
            Variable::Tag => &self.tag,
            Variable::Event => &self.event,
            Variable::Sha => &self.sha,
        }
    }
}

/// A parsed `when` expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    /// A single `variable op 'value'` test.
    Compare {
        /// Which of the four variables is being tested.
        variable: Variable,
        /// `==` or `!=`.
        op: CompareOp,
        /// The literal on the right-hand side.
        value: String,
    },
    /// Both sides must hold.
    And(Box<Condition>, Box<Condition>),
    /// Either side may hold.
    Or(Box<Condition>, Box<Condition>),
}

impl Condition {
    /// Parses a `when` expression, or fails with a position-aware [`ConditionError`].
    pub fn parse(source: &str) -> Result<Self, ConditionError> {
        let tokens = tokenize(source)?;
        if tokens.is_empty() {
            return Err(ConditionError::Empty);
        }

        let mut parser = Parser { tokens, at: 0 };
        let condition = parser.expression()?;
        match parser.peek() {
            Some(token) => Err(ConditionError::Trailing {
                token: token.describe(),
            }),
            None => Ok(condition),
        }
    }

    /// Whether this condition holds against a specific run.
    pub fn evaluate(&self, context: &EvalContext) -> bool {
        match self {
            Self::Compare {
                variable,
                op,
                value,
            } => {
                let actual = context.get(*variable);
                match op {
                    CompareOp::Eq => actual == value,
                    CompareOp::Ne => actual != value,
                }
            }
            Self::And(left, right) => left.evaluate(context) && right.evaluate(context),
            Self::Or(left, right) => left.evaluate(context) || right.evaluate(context),
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compare {
                variable,
                op,
                value,
            } => write!(f, "{variable} {} '{value}'", op.as_str()),
            Self::And(left, right) => write!(f, "{left} && {right}"),
            Self::Or(left, right) => write!(f, "{left} || {right}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a `when` expression failed to parse.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConditionError {
    /// The `when` value was an empty string.
    #[error("condition is empty")]
    Empty,

    /// An identifier isn't one of [`Variable::ALL`].
    #[error("unknown variable '{name}' (known: {known})")]
    UnknownVariable {
        /// The identifier that was written.
        name: String,
        /// The variables that are actually valid, comma-joined.
        known: String,
    },

    /// A comparison didn't start with a variable name.
    #[error("expected a variable, found {found}")]
    ExpectedVariable {
        /// What was found instead, already formatted for display.
        found: String,
    },

    /// A variable wasn't followed by `==` or `!=`.
    #[error("expected '==' or '!=' after '{variable}', found {found}")]
    ExpectedComparison {
        /// The variable the comparison operator was expected after.
        variable: String,
        /// What was found instead, already formatted for display.
        found: String,
    },

    /// A comparison operator wasn't followed by a quoted literal.
    #[error("expected a quoted value after '{op}', found {found}")]
    ExpectedValue {
        /// The operator the value was expected after.
        op: String,
        /// What was found instead, already formatted for display.
        found: String,
    },

    /// Tokens remained after a complete expression was parsed.
    #[error("unexpected {token} at the end of the condition")]
    Trailing {
        /// The first leftover token, already formatted for display.
        token: String,
    },

    /// A quoted literal had no matching closing quote.
    #[error("unterminated string: no closing {quote}")]
    UnterminatedString {
        /// The quote character (`'` or `"`) that was never closed.
        quote: char,
    },

    /// A character doesn't start any valid token.
    #[error("unexpected character '{found}'{}", hint(*found))]
    UnexpectedCharacter {
        /// The offending character.
        found: char,
    },
}

/// Nudges the two mistakes people actually make: writing `=` for `==`, and
/// bringing `and`/`or` in from another CI tool's syntax.
fn hint(found: char) -> &'static str {
    match found {
        '=' => " (comparison is '==', not '=')",
        '&' => " (conjunction is '&&')",
        '|' => " (disjunction is '||')",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    Str(String),
    Op(CompareOp),
    And,
    Or,
}

impl Token {
    /// How the token is named in an error message.
    fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("'{name}'"),
            Self::Str(value) => format!("'{value}'"),
            Self::Op(op) => format!("'{}'", op.as_str()),
            Self::And => "'&&'".to_string(),
            Self::Or => "'||'".to_string(),
        }
    }
}

fn tokenize(source: &str) -> Result<Vec<Token>, ConditionError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;

    while at < chars.len() {
        let current = chars[at];

        if current.is_whitespace() {
            at += 1;
            continue;
        }

        // A quoted literal. There are no escapes: a value containing a quote is
        // not something a branch name or an event can be, and supporting them
        // would be the first step towards a string language nobody asked for.
        if current == '\'' || current == '"' {
            let quote = current;
            at += 1;
            let start = at;
            while at < chars.len() && chars[at] != quote {
                at += 1;
            }
            if at >= chars.len() {
                return Err(ConditionError::UnterminatedString { quote });
            }
            tokens.push(Token::Str(chars[start..at].iter().collect()));
            at += 1;
            continue;
        }

        if current.is_alphanumeric() || current == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            tokens.push(Token::Ident(chars[start..at].iter().collect()));
            continue;
        }

        let next = chars.get(at + 1).copied();
        match (current, next) {
            ('=', Some('=')) => {
                tokens.push(Token::Op(CompareOp::Eq));
                at += 2;
            }
            ('!', Some('=')) => {
                tokens.push(Token::Op(CompareOp::Ne));
                at += 2;
            }
            ('&', Some('&')) => {
                tokens.push(Token::And);
                at += 2;
            }
            ('|', Some('|')) => {
                tokens.push(Token::Or);
                at += 2;
            }
            _ => return Err(ConditionError::UnexpectedCharacter { found: current }),
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        if token.is_some() {
            self.at += 1;
        }
        token
    }

    fn describe_next(&self) -> String {
        self.peek()
            .map_or_else(|| "the end of the condition".to_string(), Token::describe)
    }

    fn expression(&mut self) -> Result<Condition, ConditionError> {
        let mut left = self.conjunction()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.at += 1;
            let right = self.conjunction()?;
            left = Condition::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn conjunction(&mut self) -> Result<Condition, ConditionError> {
        let mut left = self.comparison()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.at += 1;
            let right = self.comparison()?;
            left = Condition::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Condition, ConditionError> {
        let found = self.describe_next();
        let Some(Token::Ident(name)) = self.next() else {
            return Err(ConditionError::ExpectedVariable { found });
        };

        let variable = Variable::parse(&name).ok_or_else(|| ConditionError::UnknownVariable {
            name: name.clone(),
            known: Variable::known(),
        })?;

        let found = self.describe_next();
        let Some(Token::Op(op)) = self.next() else {
            return Err(ConditionError::ExpectedComparison {
                variable: name,
                found,
            });
        };

        let found = self.describe_next();
        let Some(Token::Str(value)) = self.next() else {
            return Err(ConditionError::ExpectedValue {
                op: op.as_str().to_string(),
                found,
            });
        };

        Ok(Condition::Compare {
            variable,
            op,
            value,
        })
    }
}
