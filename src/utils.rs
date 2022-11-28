#[derive(Clone, Debug)]
pub struct Card {
    pub suit: String,
    pub value: i32,
}
pub fn format_card(card: &Card) -> String {
    let mut card_str = String::new();
    match card.value {
        11 => card_str.push_str("J"),
        12 => card_str.push_str("Q"),
        13 => card_str.push_str("K"),
        14 => card_str.push_str("A"),
        _ => card_str.push_str(&card.value.to_string()),
    }
    card_str.push_str(&card.suit);
    return card_str;
}
