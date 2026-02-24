/// Format a CoreAudio `OSStatus` code into a human-readable string.
///
/// Known error codes are returned with their symbolic name; unknown codes
/// are formatted as hex.
pub fn os_status(code: i32) -> String {
    match code {
        0 => "kAudioHardwareNoError (0)".to_string(),
        -1500 => "kAudioHardwareUnspecifiedError (-1500)".to_string(),
        -1501 => "kAudioHardwareNotRunningError (-1501)".to_string(),
        -1502 => "kAudioHardwareUnknownPropertyError (-1502)".to_string(),
        -1503 => "kAudioHardwareBadPropertySizeError (-1503)".to_string(),
        -1504 => "kAudioHardwareIllegalOperationError (-1504)".to_string(),
        -1505 => "kAudioHardwareBadObjectError (-1505)".to_string(),
        -1506 => "kAudioHardwareBadDeviceError (-1506)".to_string(),
        -1507 => "kAudioHardwareBadStreamError (-1507)".to_string(),
        other => format!("OSStatus {other} ({other:#X})"),
    }
}

/// Build a CoreAudio error string from an operation description and `OSStatus`.
pub fn ca_error(operation: &str, code: i32) -> String {
    format!("CoreAudio {operation} failed: {}", os_status(code))
}
