//! Hand scoring: the one place that knows an ace is worth 1 or 11.

use crate::card::Card;

/// A scored hand. `soft` means an ace is still counted as 11, so the hand
/// cannot bust on the next card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Total {
    pub value: u16,
    pub soft: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hand {
    cards: Vec<Card>,
}

impl Hand {
    pub fn from_cards(cards: Vec<Card>) -> Self {
        Self { cards }
    }

    pub fn push(&mut self, card: Card) {
        self.cards.push(card);
    }

    pub fn clear(&mut self) {
        self.cards.clear();
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// Count every ace as 11, then demote one ace at a time while the hand is
    /// over 21. This is O(cards) and needs no search over ace assignments:
    /// only one ace can ever be worth 11 in a live hand, because two would add
    /// 22 on their own.
    pub fn total(&self) -> Total {
        let mut value: u16 = self.cards.iter().map(|card| card.rank.points()).sum();
        let mut elevens = self.cards.iter().filter(|card| card.rank.is_ace()).count();
        while value > 21 && elevens > 0 {
            value -= 10;
            elevens -= 1;
        }
        Total { value, soft: elevens > 0 }
    }

    /// A natural: 21 on the opening two cards. Three cards totalling 21 are
    /// not a blackjack and are not paid at 3:2.
    pub fn is_blackjack(&self) -> bool {
        self.cards.len() == 2 && self.total().value == 21
    }

    pub fn is_bust(&self) -> bool {
        self.total().value > 21
    }

    pub fn holds_ace(&self) -> bool {
        self.cards.iter().any(|card| card.rank.is_ace())
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::card::{Card, Rank, Suit};
    use crate::hand::Hand;

    /// Builds a hand from ranks; suits are irrelevant to blackjack scoring.
    pub fn hand(ranks: &[Rank]) -> Hand {
        let cards = ranks.iter().map(|&rank| Card { rank, suit: Suit::Spades }).collect();
        Hand::from_cards(cards)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::hand;
    use crate::card::Rank::*;

    #[test]
    fn hand_without_an_ace_is_a_plain_sum() {
        let total = hand(&[Seven, Nine]).total();
        assert_eq!((total.value, total.soft), (16, false));
    }

    #[test]
    fn lone_ace_counts_as_eleven_and_is_soft() {
        let total = hand(&[Ace, Six]).total();
        assert_eq!((total.value, total.soft), (17, true));
    }

    #[test]
    fn soft_seventeen_hardens_to_seventeen_when_a_ten_lands() {
        let total = hand(&[Ace, Six, Ten]).total();
        assert_eq!((total.value, total.soft), (17, false));
    }

    #[test]
    fn two_aces_total_twelve_because_only_one_can_be_eleven() {
        let total = hand(&[Ace, Ace]).total();
        assert_eq!((total.value, total.soft), (12, true));
    }

    #[test]
    fn four_aces_total_fourteen_and_stay_soft() {
        let total = hand(&[Ace, Ace, Ace, Ace]).total();
        assert_eq!((total.value, total.soft), (14, true));
    }

    #[test]
    fn ace_ace_nine_is_soft_twenty_one() {
        let total = hand(&[Ace, Ace, Nine]).total();
        assert_eq!((total.value, total.soft), (21, true));
    }

    #[test]
    fn ace_ace_ten_hardens_to_twelve_rather_than_busting() {
        let total = hand(&[Ace, Ace, Ten]).total();
        assert_eq!((total.value, total.soft), (12, false));
    }

    #[test]
    fn every_ace_demotes_when_the_hand_would_otherwise_bust() {
        let total = hand(&[Ace, Ace, Ace, Eight, Ten]).total();
        assert_eq!((total.value, total.soft), (21, false));
    }

    #[test]
    fn a_hand_with_aces_can_still_bust_once_all_are_demoted() {
        let total = hand(&[Ace, Ten, Ten, Five]).total();
        assert_eq!((total.value, total.soft), (26, false));
        assert!(hand(&[Ace, Ten, Ten, Five]).is_bust());
    }

    #[test]
    fn ace_with_a_face_card_is_a_blackjack() {
        assert!(hand(&[Ace, King]).is_blackjack());
        assert!(hand(&[Queen, Ace]).is_blackjack());
    }

    #[test]
    fn twenty_one_on_three_cards_is_not_a_blackjack() {
        let three = hand(&[Seven, Seven, Seven]);
        assert_eq!(three.total().value, 21);
        assert!(!three.is_blackjack());
    }

    #[test]
    fn twenty_one_is_not_a_bust() {
        assert!(!hand(&[Ace, King]).is_bust());
        assert!(hand(&[Ten, Nine, Five]).is_bust());
    }

    #[test]
    fn empty_hand_scores_zero_and_is_neither_bust_nor_blackjack() {
        let total = hand(&[]).total();
        assert_eq!((total.value, total.soft), (0, false));
        assert!(!hand(&[]).is_bust());
        assert!(!hand(&[]).is_blackjack());
    }
}
