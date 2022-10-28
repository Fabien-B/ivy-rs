use crate::ivyerror::IvyError;


#[derive(Debug)]
pub enum IvyMsg {
    Bye,
    Sub(u32, String),
    TextMsg(u32, String),
    Error(String),
    DelSub(u32),
    EndSub,
    PeerId(u16, String),
    DirectMsg(u32, String),
    Quit,
    Ping(u32),
    Pong(u32),
}


impl IvyMsg {

    pub fn parse(buf: &[u8]) -> Result<Self, IvyError> {
        const SPACE: u8 = ' ' as u8;
        if let [t, SPACE , right@..] = buf {
            let msg_id = (*t as char).to_digit(10).unwrap();
            if let [ident, params] =  right.split(|c| *c == 0x02).collect::<Vec<_>>()[..] {
                let ident = match std::str::from_utf8(ident).unwrap().parse::<u32>() {
                    Ok(n) => Ok(n),
                    Err(_) => Err(IvyError::ParseFail),
                }?;
                //let params = params.split(pred)
                println!("recv msg {} with ident {}, params: {:?}", msg_id, ident, params);

            }
        }
        Err(IvyError::ParseFail)
    }

    pub fn format_ivy(t: u8, ident: u32, params: &str) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        buffer.extend(format!("{} {}", t, ident).as_bytes());
        buffer.push(2);
        buffer.extend(String::from(params).as_bytes());
        buffer.push('\n' as u8);
        buffer
    }

    pub fn to_ascii(&self) -> Vec<u8>  {

        match self {
            IvyMsg::Bye => IvyMsg::format_ivy(u8::from(self), 0, ""),
            IvyMsg::Sub(ident, reg) => IvyMsg::format_ivy(u8::from(self), *ident, reg),
            IvyMsg::TextMsg(ident, params) => IvyMsg::format_ivy(u8::from(self), *ident, params),
            IvyMsg::Error(_) => todo!(),
            IvyMsg::DelSub(_) => todo!(),
            IvyMsg::EndSub => IvyMsg::format_ivy(u8::from(self), 0, ""),
            IvyMsg::PeerId(_, _) => todo!(),
            IvyMsg::DirectMsg(_, _) => todo!(),
            IvyMsg::Quit => todo!(),
            IvyMsg::Ping(_) => todo!(),
            IvyMsg::Pong(_) => todo!(),
        }
    }
}

impl From<&IvyMsg> for u8 {
    fn from(value: &IvyMsg) -> Self {
        match value {
            IvyMsg::Bye => 0,
            IvyMsg::Sub(_, _) => 1,
            IvyMsg::TextMsg(_, _) => 2,
            IvyMsg::Error(_) => 3,
            IvyMsg::DelSub(_) => 4,
            IvyMsg::EndSub => 5,
            IvyMsg::PeerId(_, _) => 6,
            IvyMsg::DirectMsg(_, _) => 7,
            IvyMsg::Quit => 8,
            IvyMsg::Ping(_) => 9,
            IvyMsg::Pong(_) => 10,
            
        }
    }
}
