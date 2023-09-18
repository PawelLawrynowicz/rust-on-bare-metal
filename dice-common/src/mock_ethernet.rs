#[cfg(test)]
//This is modified code of an example of device trait implementation from smoltcp documentation.
//used as a placeholder for testing

//It returns one icmp echo request

extern crate std;

use smoltcp::wire::EthernetAddress;
use smoltcp::wire::Ipv4Packet;
use smoltcp::phy::{Device, DeviceCapabilities};
use smoltcp::time::Instant;
use smoltcp::wire::{Ipv4Repr, EthernetRepr, EthernetProtocol, EthernetFrame};
use smoltcp::Result;

use smoltcp::wire::{Icmpv4Packet, Icmpv4Repr, IpProtocol, Ipv4Address};

const MAX_ICMP_MESSAGE_SIZE: usize = 576;
const TEST_DATA: [u8; 4] = [1, 2, 3, 4];

static mut COUNTER: u16 = 1;

pub struct StmPhy {
    rx_buffer: [u8; 1536],
    tx_buffer: [u8; 1536],
}

impl<'a> StmPhy {
    pub fn new() -> StmPhy {
        let mut device = StmPhy {
            rx_buffer: [0; 1536],
            tx_buffer: [0; 1536],
        };

        let dev_cap = device.capabilities();

        let icmp_repr = Icmpv4Repr::EchoRequest {
            ident: 1,
            seq_no: unsafe { COUNTER },
            data: &TEST_DATA,
        };

        let mut icmp_payload = [0; MAX_ICMP_MESSAGE_SIZE];
        let mut icmp_packet = Icmpv4Packet::new_unchecked(&mut icmp_payload);
        icmp_repr.emit(&mut icmp_packet, &dev_cap.checksum);

        let icmp_inner = icmp_packet.into_inner();

        let ip_repr = Ipv4Repr {
            src_addr: Ipv4Address::new(192, 168, 1, 1),
            dst_addr: Ipv4Address::new(192, 168, 1, 2),
            protocol: IpProtocol::Icmp,
            payload_len: MAX_ICMP_MESSAGE_SIZE,
            hop_limit: 20,
        };

        let mut ip_buffer = [0; 60 + MAX_ICMP_MESSAGE_SIZE];
        let mut ip_packet = Ipv4Packet::new_unchecked(&mut ip_buffer);

        ip_repr.emit(&mut ip_packet, &dev_cap.checksum);

        let mut payload = ip_packet.payload_mut();
        
        for i in 0..MAX_ICMP_MESSAGE_SIZE {
            payload[i] = icmp_inner[i];
        }
        
        let ip_inner = ip_packet.into_inner();
        
        let mut eth_repr = EthernetRepr{
            src_addr: EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]),
            dst_addr: EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]),
            ethertype: EthernetProtocol::Ipv4,
        };
        
        let mut eth_buffer = [0; 1536];

        let mut frame = EthernetFrame::new_unchecked(&mut device.rx_buffer);
        eth_repr.emit(&mut frame);
        
        let mut payload = frame.payload_mut();
        
        for i in 0..60 + MAX_ICMP_MESSAGE_SIZE {
            payload[i] = ip_inner[i];
        }
        
        device
    }
}

impl<'a> smoltcp::phy::Device<'a> for StmPhy {
    type RxToken = StmPhyRxToken<'a>;
    type TxToken = StmPhyTxToken<'a>;

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        unsafe{
            if COUNTER < 2  {
                COUNTER+=1; 
                Some((
                    StmPhyRxToken(&mut self.rx_buffer[..]),
                    StmPhyTxToken(&mut self.tx_buffer[..]),
                ))
            }else{
                
                None     
            }
        }
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        //Some(StmPhyTxToken(&mut self.tx_buffer[..]))
        None
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(1);
        caps
    }
}

pub struct StmPhyRxToken<'a>(&'a mut [u8]);

impl<'a> smoltcp::phy::RxToken for StmPhyRxToken<'a> {
    fn consume<R, F>(mut self, _timestamp: Instant, f: F) -> Result<R>
    where
        F: FnOnce(&mut [u8]) -> Result<R>,
    {
        //receive packet into buffer
        let result = f(&mut self.0);
        result
    }
}

pub struct StmPhyTxToken<'a>(&'a mut [u8]);

impl<'a> smoltcp::phy::TxToken for StmPhyTxToken<'a> {
    fn consume<R, F>(self, _timestamp: Instant, len: usize, f: F) -> Result<R>
    where
        F: FnOnce(&mut [u8]) -> Result<R>,
    {
        let result = f(&mut self.0[..len]);
        //send packet out
        result
    }
}
