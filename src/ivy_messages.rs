use crate::ivyerror::IvyError;


#[derive(Debug)]
pub enum IvyMsg {
    Bye,
    Sub(u32, String),
    TextMsg(u32, Vec<String>),
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

    pub fn new(msg_id: u32, ident: u32, params: &[u8]) -> Option<Self> {
        match msg_id {
            0 => Some(Self::Bye),
            1 => Some(Self::Sub(ident, std::str::from_utf8(params).unwrap().into())),
            2 => {
                let params = params
                    .split(|c| *c == 0x03)
                    .map(|buf| std::str::from_utf8(buf).unwrap().into())
                    .collect();
                Some(Self::TextMsg(ident, params))
            },
            3 => Some(Self::Error(std::str::from_utf8(params).unwrap().into())),
            4 => Some(Self::DelSub(ident)),
            5 => Some(Self::EndSub),
            6 => Some(Self::PeerId(ident as u16, std::str::from_utf8(params).unwrap().into())),
            7 => Some(Self::DirectMsg(ident, std::str::from_utf8(params).unwrap().into())),
            8 => Some(Self::Quit),
            9 => Some(Self::Ping(ident)),
            10 => Some(Self::Pong(ident)),
            _ => None
        }
    }

    pub fn parse(buf: &[u8]) -> Result<Self, IvyError> {
        
        if let [msg_id, ident, params] = buf.splitn(3, |c| *c == ' ' as u8 || *c == 0x02).collect::<Vec<_>>()[..] {
            let msg_id = std::str::from_utf8(msg_id).unwrap().parse::<u32>()?;
            let ident = std::str::from_utf8(ident).unwrap().parse::<u32>()?;
            let ivy_msg = Self::new(msg_id, ident, params);
            if let Some(ivy_msg) = ivy_msg {
                return Ok(ivy_msg);
            }
        }
        Err(IvyError::ParseFail)
    }

    pub fn format_ivy(t: u8, ident: u32, params: &[u8]) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        buffer.extend(format!("{} {}", t, ident).as_bytes());
        buffer.push(2);
        buffer.extend(params);
        buffer.push('\n' as u8);
        buffer
    }

    pub fn to_ascii(&self) -> Vec<u8>  {
        match self {
            IvyMsg::Bye => IvyMsg::format_ivy(u8::from(self), 0, &[]),
            IvyMsg::Sub(ident, reg) => IvyMsg::format_ivy(u8::from(self), *ident, reg.as_bytes()),
            IvyMsg::TextMsg(ident, params) => {
                let params = params
                    .iter()
                    .map(|p| p.as_bytes())
                    .fold(Vec::<u8>::new(), |mut acc, p| {
                        acc.extend(p);
                        acc.push(0x03);
                        acc
                    });
                IvyMsg::format_ivy(u8::from(self), *ident, params.as_slice())
            },
            IvyMsg::Error(txt) => IvyMsg::format_ivy(u8::from(self), 0, txt.as_bytes()),
            IvyMsg::DelSub(sub_id) => IvyMsg::format_ivy(u8::from(self), *sub_id, &[]),
            IvyMsg::EndSub => IvyMsg::format_ivy(u8::from(self), 0, &[]),
            IvyMsg::PeerId(port, app_name) => IvyMsg::format_ivy(u8::from(self), *port as u32, app_name.as_bytes()),
            IvyMsg::DirectMsg(ident, msg) => IvyMsg::format_ivy(u8::from(self), *ident, msg.as_bytes()),
            IvyMsg::Quit => IvyMsg::format_ivy(u8::from(self), 0, &[]),
            IvyMsg::Ping(ping_id) => IvyMsg::format_ivy(u8::from(self), *ping_id, &[]),
            IvyMsg::Pong(ping_id) => IvyMsg::format_ivy(u8::from(self), *ping_id, &[]),
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


/// Parse UDP annoucement message
/// 
/// Returns a tuple `(protocol_version: u8, port: u16, watcher_id: String, peer_name: String)`
///
pub fn parse_udp_announce(annouce: &str) -> Result<(u32, u16, String, String), IvyError> {
    let splitted: Vec<_> = annouce.split(" ").collect();
    if splitted.len() != 4 {
        return Err(IvyError::ParseAnnounceError);
    }
    let protocol_version = splitted[0].parse()?;
    let port = splitted[1].parse()?;
    let watcher_id = splitted[2]; 
    let peer_name = splitted[3]; 

    Ok((protocol_version, port, watcher_id.to_owned(), peer_name.to_owned()))
}
