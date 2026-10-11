#[cfg(test)]
pub fn submit_matrix(socket: &std::path::Path, source: &str) -> Result<(), String> {
    let mut client = mote_ipc::Client::connect(socket)
        .map_err(|error| format!("cannot reach daemon: {error}"))?;
    submit_matrix_with_client(&mut client, source)
}

pub fn submit_matrix_for_host(
    host: &crate::stream::UnixHostSession,
    source: &str,
) -> Result<(), String> {
    let mut client = host.verified_client().map_err(|error| error.message)?;
    submit_matrix_with_client(&mut client, source)
}

fn submit_matrix_with_client(client: &mut mote_ipc::Client, source: &str) -> Result<(), String> {
    let response = client
        .call(
            "matrix.render",
            serde_json::json!({"thread":"main", "source":source}),
        )
        .map_err(|error| format!("cannot submit matrix source: {error}"))?
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    if response["ok"].as_bool() == Some(true)
        || (response["ok"].as_bool() == Some(false) && response["diagnostics"].is_array())
    {
        Ok(())
    } else {
        Err(format!(
            "matrix source rejected: {}",
            response["diagnostics"]
        ))
    }
}
