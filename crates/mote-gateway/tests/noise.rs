use mote_gateway::noise::{hex, seal, unhex, Opener};
use serde_json::Value;
use snow::Builder;

fn check_vectors(text: &str) {
    let vectors: Value = serde_json::from_str(text).unwrap();
    for v in vectors["vectors"].as_array().unwrap() {
        let bytes = |key: &str| unhex(v[key].as_str().unwrap()).unwrap();
        let (is, ie, rs, re, rp) = (
            bytes("initiator_static_private"),
            bytes("initiator_ephemeral_private"),
            bytes("responder_static_private"),
            bytes("responder_ephemeral_private"),
            bytes("responder_static_public"),
        );
        let prologue = v["prologue"].as_str().unwrap().as_bytes();
        let pattern = v["pattern"].as_str().unwrap();
        let mut ib = Builder::new(pattern.parse().unwrap())
            .prologue(prologue)
            .unwrap()
            .local_private_key(&is)
            .unwrap()
            .remote_public_key(&rp)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&ie);
        let mut rb = Builder::new(pattern.parse().unwrap())
            .prologue(prologue)
            .unwrap()
            .local_private_key(&rs)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&re);
        let psk = v["psk"]
            .as_str()
            .map(|s| <[u8; 32]>::try_from(unhex(s).unwrap()).unwrap());
        if let Some(psk) = &psk {
            ib = ib.psk(1, psk).unwrap();
            rb = rb.psk(1, psk).unwrap();
        }
        let (mut i, mut r) = (ib.build_initiator().unwrap(), rb.build_responder().unwrap());
        let mut out = [0; 4096];
        let n = i
            .write_message(v["payload1"].as_str().unwrap().as_bytes(), &mut out)
            .unwrap();
        assert_eq!(hex(&out[..n]), v["message1"].as_str().unwrap());
        let mut plain = [0; 4096];
        r.read_message(&out[..n], &mut plain).unwrap();
        let n = r
            .write_message(v["payload2"].as_str().unwrap().as_bytes(), &mut out)
            .unwrap();
        assert_eq!(hex(&out[..n]), v["message2"].as_str().unwrap());
        i.read_message(&out[..n], &mut plain).unwrap();
        assert_eq!(
            hex(i.get_handshake_hash()),
            v["handshake_hash"].as_str().unwrap()
        );
        let (mut it, mut rt) = (
            i.into_transport_mode().unwrap(),
            r.into_transport_mode().unwrap(),
        );
        for t in v["transport"].as_array().unwrap() {
            let (tx, rx) = if t["from"] == "device" {
                (&mut it, &mut rt)
            } else {
                (&mut rt, &mut it)
            };
            let text = t["plaintext"].as_str().unwrap();
            let frames = seal(tx, text.as_bytes()).unwrap();
            assert_eq!(
                frames.iter().map(|f| hex(f)).collect::<Vec<_>>(),
                t["frames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| f.as_str().unwrap().to_string())
                    .collect::<Vec<_>>()
            );
            let mut opener = Opener::new(1 << 20);
            let mut joined = None;
            for frame in frames {
                joined = opener.open(rx, &frame).unwrap();
            }
            assert_eq!(joined.unwrap(), text.as_bytes());
        }
    }
}

#[test]
fn approved_overseer_vectors_stay_compatible() {
    check_vectors(include_str!("vectors/overseer-noise.json"));
}

#[test]
fn mote_vectors_bind_the_mote_prologue() {
    check_vectors(include_str!("vectors/mote-noise.json"));
}
