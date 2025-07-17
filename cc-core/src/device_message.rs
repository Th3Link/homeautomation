#[derive(Debug, Copy, Clone)]
pub struct IdTypeMsg {}

impl IdTypeMsg {
    pub fn parse(data: &[u8]) -> Option<(u8, u8)> {
        if data.len() != 2 {
            return None;
        }

        let id = data[0];
        let dtype = data[1];
        Some((id, dtype))
    }
}
