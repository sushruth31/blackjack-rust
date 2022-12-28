//! Cards, ranks, suits, and the shoe they are dealt from.

use rand::Rng;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}

impl Suit {
    pub const ALL: [Self; 4] = [Self::Spades, Self::Hearts, Self::Diamonds, Self::Clubs];
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let glyph = match self {
            Self::Spades => '\u{2660}',
            Self::Hearts => '\u{2665}',
            Self::Diamonds => '\u{2666}',
            Self::Clubs => '\u{2663}',
        };
        write!(f, "{}", glyph)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl Rank {
    pub const ALL: [Self; 13] = [
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
        Self::Ace,
    ];

    /// Score contribution with an ace taken at its maximum value.
    ///
    /// An ace is the only rank worth two amounts, so it enters scoring at 11
    /// and [`crate::hand::Hand::total`] demotes it to 1 when the hand needs it.
    /// Keeping that single rule in one place is why nothing else has to know
    /// that aces are special.
    pub fn points(self) -> u16 {
        match self {
            Self::Ace => 11,
            Self::Ten | Self::Jack | Self::Queen | Self::King => 10,
            pip => pip as u16 + 2,
        }
    }

    pub fn is_ace(self) -> bool {
        self == Self::Ace
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ace => write!(f, "A"),
            Self::King => write!(f, "K"),
            Self::Queen => write!(f, "Q"),
            Self::Jack => write!(f, "J"),
            pip => write!(f, "{}", pip.points()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank, self.suit)
    }
}

/// A shoe of one or more standard 52-card packs. Cards are drawn from the back.
#[derive(Clone, Debug)]
pub struct Deck(Vec<Card>);

impl Deck {
    /// An unshuffled shoe of `packs` standard packs.
    pub fn new(packs: u8) -> Self {
        let pack: Vec<Card> = Suit::ALL
            .iter()
            .flat_map(|&suit| Rank::ALL.map(move |rank| Card { rank, suit }))
            .collect();
        Self(pack.repeat(packs as usize))
    }

    /// Fisher-Yates: card `i` swaps with a card drawn from `0..=i`, never from
    /// the whole shoe. Sampling the full range instead produces `n^n` equally
    /// likely swap sequences over `n!` permutations, and `n!` does not divide
    /// `n^n`, so some orderings come up more often than others.
    pub fn shuffle<R: Rng>(&mut self, rng: &mut R) {
        for i in (1..self.0.len()).rev() {
            self.0.swap(i, rng.gen_range(0..=i));
        }
    }

    pub fn draw(&mut self) -> Option<Card> {
        self.0.pop()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn counted(deck: &Deck, rank: Rank) -> usize {
        deck.0.iter().filter(|card| card.rank == rank).count()
    }

    #[test]
    fn single_pack_holds_52_cards_and_4_of_every_rank() {
        let deck = Deck::new(1);
        assert_eq!(deck.len(), 52);
        assert!(Rank::ALL.iter().all(|&rank| counted(&deck, rank) == 4));
    }

    #[test]
    fn six_pack_shoe_holds_96_ten_point_cards() {
        let deck = Deck::new(6);
        let tens = deck.0.iter().filter(|card| card.rank.points() == 10).count();
        assert_eq!(deck.len(), 312);
        assert_eq!(tens, 96);
    }

    #[test]
    fn faces_are_worth_ten_and_the_ace_enters_at_eleven() {
        assert_eq!(Rank::Jack.points(), 10);
        assert_eq!(Rank::Queen.points(), 10);
        assert_eq!(Rank::King.points(), 10);
        assert_eq!(Rank::Ace.points(), 11);
        assert_eq!(Rank::Two.points(), 2);
        assert_eq!(Rank::Ten.points(), 10);
    }

    #[test]
    fn shuffle_permutes_without_adding_or_losing_a_card() {
        let ordered = Deck::new(2);
        let mut shuffled = ordered.clone();
        shuffled.shuffle(&mut StdRng::seed_from_u64(7));
        let count = |deck: &Deck, card: &Card| deck.0.iter().filter(|c| *c == card).count();
        assert_eq!(shuffled.len(), ordered.len());
        assert!(ordered.0.iter().all(|c| count(&shuffled, c) == count(&ordered, c)));
        assert_ne!(shuffled.0, ordered.0);
    }

    #[test]
    fn shuffle_is_reproducible_for_a_given_seed() {
        let shuffle = |seed| {
            let mut deck = Deck::new(1);
            deck.shuffle(&mut StdRng::seed_from_u64(seed));
            deck
        };
        assert_eq!(shuffle(42).0, shuffle(42).0);
        assert_ne!(shuffle(42).0, shuffle(43).0);
    }

    #[test]
    fn drawing_empties_the_shoe_and_then_yields_none() {
        let mut deck = Deck::new(1);
        for _ in 0..52 {
            assert!(deck.draw().is_some());
        }
        assert!(deck.is_empty());
        assert_eq!(deck.draw(), None);
    }

    #[test]
    fn cards_render_as_rank_then_suit() {
        let card = Card { rank: Rank::Ace, suit: Suit::Spades };
        assert_eq!(card.to_string(), "A\u{2660}");
    }
}
