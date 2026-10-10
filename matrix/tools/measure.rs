use std::error::Error;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let tokenizer = tiktoken_rs::o200k_base()?;
    let examples = fs::read_to_string("matrix/examples.md")?;
    let suite = examples
        .split("## Conformance suite")
        .nth(1)
        .ok_or("missing conformance suite")?
        .split("## Real world:")
        .next()
        .ok_or("missing suite end")?;
    let mut cases = Vec::new();
    for heading in suite.split("### ").skip(1) {
        let number: usize = heading
            .split('.')
            .next()
            .ok_or("missing case number")?
            .parse()?;
        let fenced = heading
            .split("```\n")
            .nth(1)
            .ok_or("missing source fence")?;
        let source = fenced.split("```").next().ok_or("missing fence end")?;
        cases.push((number, source.to_string()));
    }
    if cases.len() != 10 {
        return Err("expected ten design cases".into());
    }
    let mut bytes = 0;
    let mut tokens = 0;
    println!("design_case\tmatrix_bytes\tmatrix_tokens\tcompact_json_bytes\tjson_tokens");
    for (number, source) in cases {
        let source_tokens = tokenizer.encode_ordinary(&source).len();
        bytes += source.len();
        tokens += source_tokens;
        if number == 6 {
            let json: serde_json::Value =
                serde_json::from_str(&fs::read_to_string("crates/mote-ui/fixtures/04_list.json")?)?;
            let compact = serde_json::to_string(&json)?;
            println!(
                "{number}\t{}\t{source_tokens}\t{}\t{}",
                source.len(),
                compact.len(),
                tokenizer.encode_ordinary(&compact).len()
            );
        } else {
            println!("{number}\t{}\t{source_tokens}\tnone\tnone", source.len());
        }
    }
    println!("design_total\t{bytes}\t{tokens}\tpartial\tpartial");
    println!("v0_fixture\tmatrix_bytes\tmatrix_tokens\tcompact_json_bytes\tjson_tokens");
    let mut files = fs::read_dir("matrix/conformance/v0")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("matrix"));
    files.sort();
    if files.len() != 9 {
        return Err("expected nine exact v0 snapshot fragments".into());
    }
    let mut totals = [0; 4];
    for file in files {
        let stem = file
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("invalid fixture name")?;
        let source = fs::read_to_string(&file)?;
        let json_path = Path::new("crates/mote-ui/fixtures").join(format!("{stem}.json"));
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(json_path)?)?;
        let compact = serde_json::to_string(&json)?;
        let counts = [
            source.len(),
            tokenizer.encode_ordinary(&source).len(),
            compact.len(),
            tokenizer.encode_ordinary(&compact).len(),
        ];
        for (total, count) in totals.iter_mut().zip(counts) {
            *total += count;
        }
        println!(
            "{stem}\t{}\t{}\t{}\t{}",
            counts[0], counts[1], counts[2], counts[3]
        );
    }
    println!(
        "v0_total\t{}\t{}\t{}\t{}",
        totals[0], totals[1], totals[2], totals[3]
    );
    Ok(())
}
