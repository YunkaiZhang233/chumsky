//! Pratt parser for binary infix operators.
//!
//! Pratt parsing is an algorithm that allows efficient
//! parsing of binary infix operators.
//!
//! The [`Parser::pratt`] method creates a Pratt parser.
//! Its documentation contains an example of how it can be used.

mod ops;
pub use ops::{InfixOp, PrefixOp};
use ops::{Precedence, Strength};

use core::{
    cmp::{self, Ordering},
    marker::PhantomData,
};

use crate::{
    extra::ParserExtra,
    input::InputRef,
    prelude::Input,
    private::{Check, Emit, Mode, PResult, ParserSealed},
    Parser,
};

/// DOCUMENT
pub fn left_infix<P, E, PO>(parser: P, strength: u8, build: InfixBuilder<E>) -> InfixOp<P, E, PO> {
    InfixOp::new_left(parser, strength, build)
}

/// DOCUMENT
pub fn right_infix<P, E, PO>(parser: P, strength: u8, build: InfixBuilder<E>) -> InfixOp<P, E, PO> {
    InfixOp::new_right(parser, strength, build)
}

/// DOCUMENT
pub fn prefix<P, E, PO>(parser: P, strength: u8, build: PrefixBuilder<E>) -> PrefixOp<P, E, PO> {
    PrefixOp::new(parser, strength, build)
}

type InfixBuilder<E> = fn(lhs: E, rhs: E) -> E;

type PrefixBuilder<E> = fn(rhs: E) -> E;

/// DOCUMENT
pub struct PrattOpOutput<Builder>(Precedence, Builder);

/// Document
pub struct NoOps;

trait PrattParser<'a, I, Expr, E>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
{
    fn pratt_parse<M: Mode>(
        &self,
        inp: &mut InputRef<'a, '_, I, E>,
        min_strength: Option<Strength>,
    ) -> PResult<M, Expr>;
}

/// DOCUMENT
pub struct PrefixPratt<I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut> {
    pub(crate) atom: Atom,
    pub(crate) prefix_ops: PrefixOps,
    pub(crate) infix_ops: InfixOps,
    pub(crate) phantom: PhantomData<(I, O, E, PrefixOpsOut, InfixOpsOut)>,
}

impl<'a, I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut> PrattParser<'a, I, O, E>
    for PrefixPratt<I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
    Atom: Parser<'a, I, O, E>,
    InfixOps: Parser<'a, I, PrattOpOutput<InfixBuilder<O>>, E>,
    PrefixOps: Parser<'a, I, PrattOpOutput<PrefixBuilder<O>>, E>,
{
    fn pratt_parse<M: Mode>(
        &self,
        inp: &mut InputRef<'a, '_, I, E>,
        min_strength: Option<Strength>,
    ) -> PResult<M, O> {
        let pre_op = inp.save();
        let mut left = match self.prefix_ops.go::<Emit>(inp) {
            Ok(PrattOpOutput(prec, build)) => {
                let right = self.pratt_parse::<M>(inp, Some(prec.strength_right()))?;
                M::map(right, build)
            }
            Err(_) => {
                inp.rewind(pre_op);
                self.atom.go::<M>(inp)?
            }
        };

        loop {
            let pre_op = inp.save();
            let (op, prec) = match self.infix_ops.go::<Emit>(inp) {
                Ok(PrattOpOutput(prec, build)) => {
                    if prec.strength_left().is_lt(&min_strength) {
                        inp.rewind(pre_op);
                        return Ok(left);
                    }
                    (build, prec)
                }
                Err(_) => {
                    inp.rewind(pre_op);
                    return Ok(left);
                }
            };

            let right = self.pratt_parse::<M>(inp, Some(prec.strength_right()))?;
            left = M::combine(left, right, op);
        }
    }
}

impl<'a, I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut> ParserSealed<'a, I, O, E>
    for PrefixPratt<I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
    Atom: Parser<'a, I, O, E>,
    InfixOps: Parser<'a, I, PrattOpOutput<InfixBuilder<O>>, E>,
    PrefixOps: Parser<'a, I, PrattOpOutput<PrefixBuilder<O>>, E>,
    Self: PrattParser<'a, I, O, E>,
{
    fn go<M: Mode>(&self, inp: &mut InputRef<'a, '_, I, E>) -> PResult<M, O>
    where
        Self: Sized,
    {
        self.pratt_parse::<M>(inp, None)
    }

    go_extra!(O);
}

/// DOCUMENT
#[derive(Copy, Clone)]
pub struct Pratt<I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut> {
    pub(crate) atom: Atom,
    pub(crate) prefix_ops: PrefixOps,
    pub(crate) infix_ops: InfixOps,
    // pub(crate) postfix_ops: PostfixOps,
    pub(crate) phantom: PhantomData<(I, O, E, PrefixOpsOut, InfixOpsOut)>,
}

// <I, O, E, Atom, Prefix, PrefixOpsOut, InfixOps, InfixOpsOut>

impl<'a, I, O, E, Atom, NoOps, InfixOps, InfixOpsOut>
    Pratt<I, O, E, Atom, NoOps, (), InfixOps, InfixOpsOut>
{
    fn with_prefix_ops<PrefixOps, PrefixOpsOut>(
        self,
        prefix_ops: PrefixOps,
    ) -> PrefixPratt<I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut>
    where
        I: Input<'a>,
        E: ParserExtra<'a, I>,
        PrefixOps: Parser<'a, I, PrefixOpsOut, E>,
    {
        PrefixPratt {
            atom: self.atom,
            prefix_ops,
            infix_ops: self.infix_ops,
            phantom: PhantomData,
        }
    }
}

impl<'a, I, O, E, Atom, InfixOps, InfixOpsOut> PrattParser<'a, I, O, E>
    for Pratt<I, O, E, Atom, NoOps, (), InfixOps, InfixOpsOut>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
    Atom: Parser<'a, I, O, E>,
    InfixOps: Parser<'a, I, PrattOpOutput<InfixBuilder<O>>, E>,
{
    fn pratt_parse<M>(
        &self,
        inp: &mut InputRef<'a, '_, I, E>,
        min_strength: Option<Strength>,
    ) -> PResult<M, O>
    where
        M: Mode,
    {
        let mut left = self.atom.go::<M>(inp)?;
        loop {
            let pre_op = inp.save();
            let (op, prec) = match self.infix_ops.go::<Emit>(inp) {
                Ok(PrattOpOutput(prec, build)) => {
                    if prec.strength_left().is_lt(&min_strength) {
                        inp.rewind(pre_op);
                        return Ok(left);
                    }
                    (build, prec)
                }
                Err(_) => {
                    inp.rewind(pre_op);
                    return Ok(left);
                }
            };

            let right = self.pratt_parse::<M>(inp, Some(prec.strength_right()))?;
            left = M::combine(left, right, op);
        }
    }
}

impl<'a, I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut> ParserSealed<'a, I, O, E>
    for Pratt<I, O, E, Atom, PrefixOps, PrefixOpsOut, InfixOps, InfixOpsOut>
where
    I: Input<'a>,
    E: ParserExtra<'a, I>,
    Atom: Parser<'a, I, O, E>,
    Self: PrattParser<'a, I, O, E>,
{
    fn go<M: Mode>(&self, inp: &mut InputRef<'a, '_, I, E>) -> PResult<M, O>
    where
        Self: Sized,
    {
        self.pratt_parse::<M>(inp, None)
    }

    go_extra!(O);
}

#[cfg(test)]
mod tests {
    use crate::error::Error;
    use crate::extra::Err;
    use crate::prelude::{choice, end, just, Simple, SimpleSpan};
    use crate::util::MaybeRef;
    use crate::{text, ParseResult};

    use super::*;

    enum Expr {
        Literal(i64),
        Not(Box<Expr>),
        Negate(Box<Expr>),
        Add(Box<Expr>, Box<Expr>),
        Sub(Box<Expr>, Box<Expr>),
        Mul(Box<Expr>, Box<Expr>),
        Div(Box<Expr>, Box<Expr>),
    }

    impl std::fmt::Display for Expr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Literal(literal) => write!(f, "{literal}"),
                Self::Not(right) => write!(f, "(!{right})"),
                Self::Negate(right) => write!(f, "(-{right})"),
                Self::Add(left, right) => write!(f, "({left} + {right})"),
                Self::Sub(left, right) => write!(f, "({left} - {right})"),
                Self::Mul(left, right) => write!(f, "({left} * {right})"),
                Self::Div(left, right) => write!(f, "({left} / {right})"),
            }
        }
    }

    fn parser<'a>() -> impl Parser<'a, &'a str, String, Err<Simple<'a, char>>> {
        let atom = text::int(10).from_str().unwrapped().map(Expr::Literal);

        let operator = choice((
            left_infix(just('+'), 0, |l, r| Expr::Add(Box::new(l), Box::new(r))),
            left_infix(just('-'), 0, |l, r| Expr::Sub(Box::new(l), Box::new(r))),
            right_infix(just('*'), 1, |l, r| Expr::Mul(Box::new(l), Box::new(r))),
            right_infix(just('/'), 1, |l, r| Expr::Div(Box::new(l), Box::new(r))),
        ));

        atom.pratt(operator).map(|x| x.to_string())
    }

    fn complete_parser<'a>() -> impl Parser<'a, &'a str, String, Err<Simple<'a, char>>> {
        parser().then_ignore(end())
    }

    fn parse(input: &str) -> ParseResult<String, Simple<char>> {
        complete_parser().parse(input)
    }

    fn parse_partial(input: &str) -> ParseResult<String, Simple<char>> {
        parser().lazy().parse(input)
    }

    fn unexpected<'a, C: Into<Option<MaybeRef<'a, char>>>, S: Into<SimpleSpan>>(
        c: C,
        span: S,
    ) -> Simple<'a, char> {
        <Simple<_> as Error<'_, &'_ str>>::expected_found(None, c.into(), span.into())
    }

    #[test]
    fn missing_first_expression() {
        assert_eq!(parse("").into_result(), Err(vec![unexpected(None, 0..0)]))
    }

    #[test]
    fn missing_later_expression() {
        assert_eq!(parse("1+").into_result(), Err(vec![unexpected(None, 2..2)]),);
    }

    #[test]
    fn invalid_first_expression() {
        assert_eq!(
            parse("?").into_result(),
            Err(vec![unexpected(Some('?'.into()), 0..1)]),
        );
    }

    #[test]
    fn invalid_later_expression() {
        assert_eq!(
            parse("1+?").into_result(),
            Err(vec![dbg!(unexpected(Some('?'.into()), 2..3))]),
        );
    }

    #[test]
    fn invalid_operator() {
        assert_eq!(
            parse("1?").into_result(),
            Err(vec![unexpected(Some('?'.into()), 1..2)]),
        );
    }

    #[test]
    fn invalid_operator_incomplete() {
        assert_eq!(parse_partial("1?").into_result(), Ok("1".to_string()),);
    }

    #[test]
    fn complex_nesting() {
        assert_eq!(
            parse_partial("1+2*3/4*5-6*7+8-9+10").into_result(),
            Ok("(((((1 + (2 * (3 / (4 * 5)))) - (6 * 7)) + 8) - 9) + 10)".to_string()),
        );
    }

    #[test]
    fn with_prefix_ops() {
        let atom = text::int::<_, _, Err<Simple<char>>>(10)
            .from_str()
            .unwrapped()
            .map(Expr::Literal);

        let operator = choice((
            left_infix(just('+'), 0, |l, r| Expr::Add(Box::new(l), Box::new(r))),
            left_infix(just('-'), 0, |l, r| Expr::Sub(Box::new(l), Box::new(r))),
            right_infix(just('*'), 1, |l, r| Expr::Mul(Box::new(l), Box::new(r))),
            right_infix(just('/'), 1, |l, r| Expr::Div(Box::new(l), Box::new(r))),
        ));

        let parser = atom
            .pratt(operator)
            .with_prefix_ops(choice((
                prefix(just('-'), 1, |rhs| Expr::Negate(Box::new(rhs))),
                prefix(just('!'), 1, |rhs| Expr::Negate(Box::new(rhs))),
            )))
            .map(|x| x.to_string());

        assert_eq!(
            parser.parse("-1+2*3").into_result(),
            Ok("((-1) + (2 * 3))".to_string()),
        )
    }
}
