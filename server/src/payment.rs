use stripe::Client;
use crate::server::Tier;
use shared::secret::STRIPE_SECREY_KEY;

pub struct Stripe {
    pub client: Client,
    ids: [&'static str; 15]
}

impl Stripe {
    pub fn init() -> Self {
        Stripe {
            client: Client::new(STRIPE_SECREY_KEY),
            ids: [
                "price_1NkRWwIIHjUVKUoJKDZfLive",

                "price_1NkRWzIIHjUVKUoJxHTg8JJf",
                "price_1NkRX0IIHjUVKUoJCtGzm9D7",

                "price_1NkRX3IIHjUVKUoJtED7OECe",
                "price_1NkRX4IIHjUVKUoJifiEKCZu",
                "price_1NkRX5IIHjUVKUoJXE16Dbgc",

                "price_1NkRX8IIHjUVKUoJMrurcW6r",
                "price_1NkRX9IIHjUVKUoJr7a8HLzL",
                "price_1NkRXAIIHjUVKUoJxuO2GzB6",
                "price_1NkRXCIIHjUVKUoJpn3vKmSf",

                "price_1NkRXEIIHjUVKUoJRCDI7RtW",
                "price_1NkRXFIIHjUVKUoJvc1ZzN99",
                "price_1NkRXHIIHjUVKUoJIg96CRtn",
                "price_1NkRXIIIHjUVKUoJDUglTIQp",
                "price_1NkRXJIIHjUVKUoJcfo48K97"
            ]
        }
    }
    pub fn get_id(&self, tier_from: Tier, tier_to: Tier) -> &str {
        let n = tier_to.into_numeric();

        let idx: u8 = (0..n).sum::<u8>() + (tier_from.into_numeric());

        &self.ids[idx as usize]
    }
}