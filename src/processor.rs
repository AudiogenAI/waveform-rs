use rayon::prelude::*;
use std::cmp::max;
use std::io::Cursor;
use symphonia::core::audio::{AudioBufferRef, SampleBuffer};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Converts audio data to a waveform normalized between 0 and 1.
/// data: The audio data to convert.
/// samples_per_second: The number of samples per second to output, the default
/// is 100.
pub fn audio_to_waveform(
    data: Vec<u8>,
    samples_per_second: Option<u16>,
) -> Result<Vec<f32>, String> {
    let (audio_buffer, duration) = read_sample(data)?;
    let filtered_data =
        filter_data(&audio_buffer, duration, samples_per_second);
    let normalized_data = normalize_data(&filtered_data);

    Ok(normalized_data)
}

/// Converts audio data to a waveform normalized between -1 and 1.
/// data: The audio data to convert.
/// samples_per_second: The number of samples per second to output, the default
/// is 100.
pub fn audio_to_waveform_v2(
    data: Vec<u8>,
    samples_per_second: Option<u16>,
) -> Result<Vec<f32>, String> {
    let (audio_buffer, duration) = read_sample(data)?;
    let filtered_data =
        filter_data_v2(&audio_buffer, duration, samples_per_second);
    let normalized_data = normalize_data_v2(&filtered_data);

    Ok(normalized_data)
}

fn read_sample(data: Vec<u8>) -> Result<(Vec<f32>, f32), String> {
    let cursor = Cursor::new(data);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("audio"); // Generic audio extension

    let format_opts: FormatOptions = Default::default();
    let metadata_opts: MetadataOptions = Default::default();
    let decoder_opts: DecoderOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| format!("Error while probing format: {}", e))?;

    let mut format = probed.format;
    // Find the first audio track with a known (decodeable) codec.
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .expect("no supported audio tracks");

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .map_err(|e| format!("Error while creating decoder: {}", e))?;

    let track_id = track.id;
    let mut sample_count = 0;
    let mut audio_buffer = Vec::new();

    let sample_rate = track.codec_params.sample_rate.unwrap();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::ResetRequired) => {
                return Err("Reset required, not implemented".to_string())
            }
            Err(Error::IoError(_)) => {
                // If we get an IO error, assume we've reached the end of the file
                break;
            }
            Err(e) => return Err(format!("Error reading packet: {}", e)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                if audio_buf.spec().channels.count() > 2 {
                    return Err(
                        "More than two channels not supported".to_string()
                    );
                }

                sample_count += audio_buf.capacity() as u64;
                process_audio_buffer(audio_buf, &mut audio_buffer)?;
            }
            Err(Error::IoError(_)) => {
                // If we get an IO error, assume we've reached the end of the file
                break;
            }
            Err(e) => return Err(format!("Error decoding packet: {}", e)),
        }
    }

    let duration = sample_count as f32 / sample_rate as f32;

    Ok((audio_buffer, duration))
}

fn process_audio_buffer(
    audio_buf: AudioBufferRef<'_>,
    audio_buffer: &mut Vec<f32>,
) -> Result<(), String> {
    let mut sample_buf =
        SampleBuffer::new(audio_buf.capacity() as u64, *audio_buf.spec());
    sample_buf.copy_interleaved_ref(audio_buf);
    let samples = sample_buf.samples();
    audio_buffer.extend_from_slice(samples);
    Ok(())
}

fn filter_data(
    audio_buffer: &[f32],
    duration: f32,
    samples_per_second: Option<u16>,
) -> Vec<f32> {
    let samples_per_second = samples_per_second.unwrap_or(100);
    let samples = (duration * samples_per_second as f32).floor() as usize;
    let block_size = audio_buffer.len() / samples;

    audio_buffer
        .par_chunks(block_size)
        .map(|chunk| {
            chunk.iter().map(|&x| x.abs()).sum::<f32>() / chunk.len() as f32
        })
        .collect()
}

fn filter_data_v2(
    audio_buffer: &[f32],
    duration: f32,
    samples_per_second: Option<u16>,
) -> Vec<f32> {
    let samples_per_second = samples_per_second.unwrap_or(100);
    let samples = (duration * samples_per_second as f32).floor() as usize;
    let block_size = max(1, audio_buffer.len() / samples);

    audio_buffer
        .par_chunks(block_size)
        .map(|chunk| {
            chunk
                .iter()
                .copied()
                .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0)
        })
        .collect()
}

fn normalize_data(filtered_data: &[f32]) -> Vec<f32> {
    let max_value = filtered_data
        .par_iter()
        .cloned()
        .reduce(|| f32::NEG_INFINITY, f32::max);
    let multiplier = if max_value != 0.0 {
        1.0 / max_value
    } else {
        1.0
    };
    filtered_data.par_iter().map(|&n| n * multiplier).collect()
}

fn normalize_data_v2(filtered_data: &[f32]) -> Vec<f32> {
    let max_abs_value = filtered_data
        .par_iter()
        .map(|&x| x.abs())
        .reduce(|| 0.0, f32::max);

    let multiplier = if max_abs_value != 0.0 {
        1.0 / max_abs_value
    } else {
        1.0
    };

    filtered_data.par_iter().map(|&n| n * multiplier).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    fn read_file(path: &str) -> Result<Vec<u8>, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        Ok(buffer)
    }

    /// res_is_ok:
    /// * checks if the result is normalized (all values between 0 and 1)
    /// * checks if at least one value is exactly 1.0 (the maximum)
    fn res_is_ok(res: Vec<f32>) {
        assert!(res.iter().all(|&x| (0.0..=1.0).contains(&x)));
        assert!(res.iter().any(|&x| (x - 1.0).abs() < f32::EPSILON));
    }

    #[test]
    fn test_audio_to_waveform_works_with_wav() {
        let mock_sample_path = "mocks/mock-audio.wav".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }

    #[test]
    fn test_audio_to_waveform_works_with_flac() {
        let mock_sample_path = "mocks/mock-audio.flac".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }

    #[test]
    fn test_audio_to_waveform_works_with_mp3() {
        let mock_sample_path = "mocks/mock-audio.mp3".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }

    #[test]
    fn test_audio_to_waveform_works_with_aif() {
        let mock_sample_path = "mocks/mock-audio.AIF".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }

    #[test]
    fn test_audio_to_waveform_works_with_ogg() {
        let mock_sample_path = "mocks/mock-audio.ogg".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }

    #[test]
    #[ignore = "mkv is unsupported for now"]
    fn test_audio_to_waveform_works_with_mkv() {
        let mock_sample_path = "mocks/mock-audio.mkv".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }

    #[test]
    #[ignore = "webm is unsupported for now"]
    fn test_audio_to_waveform_works_with_webm() {
        let mock_sample_path = "mocks/mock-audio.webm".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform(data, None).expect("audio_to_waveform");
        res_is_ok(res);
    }
}

#[cfg(test)]
mod tests_v2 {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    fn read_file(path: &str) -> Result<Vec<u8>, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        Ok(buffer)
    }

    /// res_is_ok_v2:
    /// * checks if the result is normalized (all values between -1 and 1)
    /// * checks if at least one value has absolute value 1.0 (the maximum magnitude)
    fn res_is_ok_v2(res: Vec<f32>) {
        assert!(res.iter().all(|&x| (-1.0..=1.0).contains(&x)));
        assert!(res.iter().any(|&x| (x.abs() - 1.0).abs() < f32::EPSILON));
    }

    #[test]
    fn test_audio_to_waveform_v2_works_with_wav() {
        let mock_sample_path = "mocks/mock-audio.wav".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }

    #[test]
    fn test_audio_to_waveform_v2_works_with_flac() {
        let mock_sample_path = "mocks/mock-audio.flac".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }

    #[test]
    fn test_audio_to_waveform_v2_works_with_mp3() {
        let mock_sample_path = "mocks/mock-audio.mp3".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }

    #[test]
    fn test_audio_to_waveform_v2_works_with_aif() {
        let mock_sample_path = "mocks/mock-audio.AIF".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }

    #[test]
    fn test_audio_to_waveform_v2_works_with_ogg() {
        let mock_sample_path = "mocks/mock-audio.ogg".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }

    #[test]
    #[ignore = "mkv is unsupported for now"]
    fn test_audio_to_waveform_v2_works_with_mkv() {
        let mock_sample_path = "mocks/mock-audio.mkv".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }

    #[test]
    #[ignore = "webm is unsupported for now"]
    fn test_audio_to_waveform_v2_works_with_webm() {
        let mock_sample_path = "mocks/mock-audio.webm".to_string();
        let data = read_file(&mock_sample_path).expect("read_file");
        let res = audio_to_waveform_v2(data, None).expect("audio_to_waveform_v2");
        res_is_ok_v2(res);
    }
}
