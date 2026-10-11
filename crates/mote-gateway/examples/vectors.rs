use mote_gateway::noise::{hex, seal, unhex};
use serde_json::Value;
use sha2::{Digest, Sha256};
use snow::Builder;

fn main() {
    let mut doc: Value =
        serde_json::from_str(include_str!("../tests/vectors/overseer-noise.json")).unwrap();
    doc["spec"] = "Mote gateway protocol 1".into();
    for v in doc["vectors"].as_array_mut().unwrap() {
        v["prologue"] = "mote-gateway-v1".into();
        let key = |name: &str| unhex(v[name].as_str().unwrap()).unwrap();
        let (is, ie, rs, re, rp) = (
            key("initiator_static_private"),
            key("initiator_ephemeral_private"),
            key("responder_static_private"),
            key("responder_ephemeral_private"),
            key("responder_static_public"),
        );
        let host_id = hex(&Sha256::digest(&rp));
        let client_id = hex(&Sha256::digest(key("initiator_static_public")));
        v["payload1"] = serde_json::json!({"host_id":host_id,"client_id":client_id,"protocol_major":1,"counter":1790000000000i64,"name":"Test iPad"}).to_string().into();
        v["payload2"] =
            serde_json::json!({"host_id":host_id,"client_id":client_id,"protocol_major":1})
                .to_string()
                .into();
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
        let mut plain = [0; 4096];
        let n = i
            .write_message(v["payload1"].as_str().unwrap().as_bytes(), &mut out)
            .unwrap();
        r.read_message(&out[..n], &mut plain).unwrap();
        let m1 = hex(&out[..n]);
        let n = r
            .write_message(v["payload2"].as_str().unwrap().as_bytes(), &mut out)
            .unwrap();
        i.read_message(&out[..n], &mut plain).unwrap();
        let m2 = hex(&out[..n]);
        let hash = hex(i.get_handshake_hash());
        let (mut it, mut rt) = (
            i.into_transport_mode().unwrap(),
            r.into_transport_mode().unwrap(),
        );
        v["message1"] = m1.into();
        v["message2"] = m2.into();
        v["handshake_hash"] = hash.into();
        for t in v["transport"].as_array_mut().unwrap() {
            let tx = if t["from"] == "device" {
                &mut it
            } else {
                &mut rt
            };
            let frames = seal(tx, t["plaintext"].as_str().unwrap().as_bytes()).unwrap();
            t["frames"] = serde_json::json!(frames.iter().map(|f| hex(f)).collect::<Vec<_>>());
        }
    }
    println!("{}", serde_json::to_string_pretty(&doc).unwrap());
}
