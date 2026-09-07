use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use crate::error::{AppError, Result};

/// Media timescale in ticks per second. 1000 = milliseconds, so duration
/// matches wall-clock recording length instead of a guessed frame rate.
const TIMESCALE: u32 = 1000;

struct Sample {
    offset: u64,
    size: u32,
    key: bool,
}

pub struct VideoRecorder {
    inner: Mutex<Option<Inner>>,
}

struct Inner {
    path: String,
    file: File,
    mdat_start: u64,
    samples: Vec<Sample>,
    sps: Vec<u8>,
    pps: Vec<u8>,
    width: u32,
    height: u32,
    started: Instant,
}

impl VideoRecorder {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    pub fn status(&self) -> (bool, u64, u64, Option<String>) {
        let lock = self.inner.lock().unwrap();
        match lock.as_ref() {
            Some(inner) => (
                true,
                inner.started.elapsed().as_millis() as u64,
                inner.file.metadata().map(|m| m.len()).unwrap_or(0),
                Some(inner.path.clone()),
            ),
            None => (false, 0, 0, None),
        }
    }

    pub fn start(
        &self,
        path: &Path,
        width: u32,
        height: u32,
        sps: Vec<u8>,
        pps: Vec<u8>,
    ) -> Result<()> {
        let mut lock = self.inner.lock().unwrap();
        if lock.is_some() {
            return Err(AppError::from("already recording"));
        }
        let mut file = File::create(path)?;
        write_ftyp(&mut file)?;
        // Reserve 16 bytes: either a 64-bit mdat header, or `free`(8)+`mdat`(8)
        // for files that fit in a 32-bit box size. Patched in finalize().
        file.write_all(&[0u8; 16])?;
        let mdat_start = file.stream_position()? - 16;
        *lock = Some(Inner {
            path: path.to_string_lossy().to_string(),
            file,
            mdat_start,
            samples: Vec::new(),
            sps,
            pps,
            width: width.max(16),
            height: height.max(16),
            started: Instant::now(),
        });
        Ok(())
    }

    pub fn set_config(&self, sps: Vec<u8>, pps: Vec<u8>, width: u32, height: u32) {
        if let Some(inner) = self.inner.lock().unwrap().as_mut() {
            if !sps.is_empty() {
                inner.sps = sps;
            }
            if !pps.is_empty() {
                inner.pps = pps;
            }
            if width > 0 {
                inner.width = width;
            }
            if height > 0 {
                inner.height = height;
            }
        }
    }

    pub fn push_sample(&self, avcc: &[u8], key: bool) -> Result<()> {
        let mut lock = self.inner.lock().unwrap();
        let Some(inner) = lock.as_mut() else {
            return Ok(());
        };
        // Players cannot decode from a mid-GOP P-frame. Drop everything until
        // the first IDR so the file opens instead of looking corrupt.
        if avcc.is_empty() || (inner.samples.is_empty() && !key) {
            return Ok(());
        }
        let offset = inner.file.stream_position()?;
        inner.file.write_all(avcc)?;
        inner.samples.push(Sample {
            offset,
            size: avcc.len() as u32,
            key,
        });
        Ok(())
    }

    pub fn finish(&self) -> Result<String> {
        let mut lock = self.inner.lock().unwrap();
        let Some(mut inner) = lock.take() else {
            return Err(AppError::from("not recording"));
        };
        if inner.samples.is_empty() || inner.sps.is_empty() || inner.pps.is_empty() {
            let path = inner.path.clone();
            drop(inner.file);
            let _ = std::fs::remove_file(&path);
            return Err(AppError::from(
                "recording produced no playable video (need a keyframe; keep Rec running or wait until the picture is on screen)",
            ));
        }
        finalize(&mut inner)?;
        inner.file.flush()?;
        Ok(inner.path)
    }
}

fn finalize(inner: &mut Inner) -> Result<()> {
    let mdat_end = inner.file.stream_position()?;
    patch_mdat_header(&mut inner.file, inner.mdat_start, mdat_end)?;
    inner.file.seek(SeekFrom::End(0))?;

    let mut moov = Vec::new();
    write_moov(&mut moov, inner)?;
    inner.file.write_all(&moov)?;
    Ok(())
}

fn patch_mdat_header(file: &mut File, mdat_start: u64, mdat_end: u64) -> Result<()> {
    let total = mdat_end - mdat_start;
    file.seek(SeekFrom::Start(mdat_start))?;
    // Prefer a 32-bit mdat. A leading `free` box eats the extra 8 bytes we
    // reserved for the 64-bit header so sample offsets stay valid.
    if total.saturating_sub(8) <= u32::MAX as u64 {
        write_u32(file, 8)?;
        file.write_all(b"free")?;
        write_u32(file, (total - 8) as u32)?;
        file.write_all(b"mdat")?;
    } else {
        write_u32(file, 1)?;
        file.write_all(b"mdat")?;
        write_u64(file, total)?;
    }
    Ok(())
}

fn write_ftyp(file: &mut File) -> Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(b"isom");
    body.extend_from_slice(&0x200u32.to_be_bytes());
    for brand in [b"isom".as_slice(), b"iso2", b"avc1", b"mp41"] {
        body.extend_from_slice(brand);
    }
    let mut buf = Vec::new();
    write_box(&mut buf, *b"ftyp", &body);
    file.write_all(&buf)?;
    Ok(())
}

fn write_moov(out: &mut Vec<u8>, inner: &Inner) -> Result<()> {
    let (duration, stts_body) = media_duration_and_stts(inner);
    let mut moov = Vec::new();
    write_mvhd(&mut moov, duration);
    write_trak(&mut moov, inner, duration, &stts_body)?;
    write_box(out, *b"moov", &moov);
    Ok(())
}

fn write_mvhd(out: &mut Vec<u8>, duration: u32) {
    let mut body = Vec::new();
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&TIMESCALE.to_be_bytes());
    body.extend_from_slice(&duration.to_be_bytes());
    body.extend_from_slice(&0x00010000u32.to_be_bytes());
    body.extend_from_slice(&0x0100u16.to_be_bytes());
    body.extend_from_slice(&0u16.to_be_bytes());
    body.extend_from_slice(&0u64.to_be_bytes());
    body.extend_from_slice(&identity_matrix());
    body.extend_from_slice(&[0u8; 24]);
    body.extend_from_slice(&2u32.to_be_bytes());
    write_box(out, *b"mvhd", &body);
}

fn write_trak(out: &mut Vec<u8>, inner: &Inner, duration: u32, stts_body: &[u8]) -> Result<()> {
    let mut trak = Vec::new();
    write_tkhd(&mut trak, inner, duration);
    write_mdia(&mut trak, inner, duration, stts_body)?;
    write_box(out, *b"trak", &trak);
    Ok(())
}

fn write_tkhd(out: &mut Vec<u8>, inner: &Inner, duration: u32) {
    let mut body = Vec::new();
    body.extend_from_slice(&0x00000007u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&duration.to_be_bytes());
    body.extend_from_slice(&0u64.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&identity_matrix());
    body.extend_from_slice(&(inner.width << 16).to_be_bytes());
    body.extend_from_slice(&(inner.height << 16).to_be_bytes());
    write_box(out, *b"tkhd", &body);
}

fn write_mdia(out: &mut Vec<u8>, inner: &Inner, duration: u32, stts_body: &[u8]) -> Result<()> {
    let mut mdia = Vec::new();
    let mut mdhd = Vec::new();
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&TIMESCALE.to_be_bytes());
    mdhd.extend_from_slice(&duration.to_be_bytes());
    mdhd.extend_from_slice(&0x55c40000u32.to_be_bytes());
    write_box(&mut mdia, *b"mdhd", &mdhd);

    let mut hdlr = Vec::new();
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(b"vide");
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(b"VideoHandler\0");
    write_box(&mut mdia, *b"hdlr", &hdlr);

    let mut minf = Vec::new();
    write_box(&mut minf, *b"vmhd", &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut dinf = Vec::new();
    let mut dref = vec![0, 0, 0, 0, 0, 0, 0, 1];
    write_box(&mut dref, *b"url ", &[0, 0, 0, 1]);
    write_box(&mut dinf, *b"dref", &dref);
    write_box(&mut minf, *b"dinf", &dinf);
    write_stbl(&mut minf, inner, stts_body)?;
    write_box(&mut mdia, *b"minf", &minf);
    write_box(out, *b"mdia", &mdia);
    Ok(())
}

fn write_stbl(out: &mut Vec<u8>, inner: &Inner, stts_body: &[u8]) -> Result<()> {
    let mut stbl = Vec::new();
    write_stsd(&mut stbl, inner);
    write_box(&mut stbl, *b"stts", stts_body);

    let keys: Vec<u32> = inner
        .samples
        .iter()
        .enumerate()
        .filter(|(_, s)| s.key)
        .map(|(i, _)| (i as u32) + 1)
        .collect();
    let mut stss = vec![0, 0, 0, 0];
    stss.extend_from_slice(&(keys.len() as u32).to_be_bytes());
    for k in keys {
        stss.extend_from_slice(&k.to_be_bytes());
    }
    write_box(&mut stbl, *b"stss", &stss);

    // One sample per chunk, matching the N entries in stco/co64.
    let mut stsc = vec![0, 0, 0, 0];
    stsc.extend_from_slice(&1u32.to_be_bytes());
    stsc.extend_from_slice(&1u32.to_be_bytes());
    stsc.extend_from_slice(&1u32.to_be_bytes());
    stsc.extend_from_slice(&1u32.to_be_bytes());
    write_box(&mut stbl, *b"stsc", &stsc);

    let mut stsz = vec![0, 0, 0, 0, 0, 0, 0, 0];
    stsz.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
    for s in &inner.samples {
        stsz.extend_from_slice(&s.size.to_be_bytes());
    }
    write_box(&mut stbl, *b"stsz", &stsz);

    let needs_co64 = inner
        .samples
        .iter()
        .any(|s| s.offset > u32::MAX as u64);
    if needs_co64 {
        let mut co64 = vec![0, 0, 0, 0];
        co64.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
        for s in &inner.samples {
            co64.extend_from_slice(&s.offset.to_be_bytes());
        }
        write_box(&mut stbl, *b"co64", &co64);
    } else {
        let mut stco = vec![0, 0, 0, 0];
        stco.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
        for s in &inner.samples {
            stco.extend_from_slice(&(s.offset as u32).to_be_bytes());
        }
        write_box(&mut stbl, *b"stco", &stco);
    }

    write_box(out, *b"stbl", &stbl);
    Ok(())
}

fn write_stsd(out: &mut Vec<u8>, inner: &Inner) {
    let mut avcc = vec![1];
    if inner.sps.len() >= 4 {
        avcc.extend_from_slice(&inner.sps[1..4]);
    } else {
        avcc.extend_from_slice(&[0x64, 0x00, 0x28]);
    }
    avcc.push(0xff);
    avcc.push(0xe1);
    avcc.extend_from_slice(&(inner.sps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(&inner.sps);
    avcc.push(1);
    avcc.extend_from_slice(&(inner.pps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(&inner.pps);

    let mut avc1 = vec![0u8; 78];
    avc1[6..8].copy_from_slice(&1u16.to_be_bytes());
    avc1[24..26].copy_from_slice(&(inner.width as u16).to_be_bytes());
    avc1[26..28].copy_from_slice(&(inner.height as u16).to_be_bytes());
    avc1[28..32].copy_from_slice(&0x00480000u32.to_be_bytes());
    avc1[32..36].copy_from_slice(&0x00480000u32.to_be_bytes());
    avc1[40..42].copy_from_slice(&1u16.to_be_bytes());
    avc1[74..76].copy_from_slice(&0x0018u16.to_be_bytes());
    avc1[76..78].copy_from_slice(&(-1i16).to_be_bytes());
    let mut avc1_full = avc1;
    write_box(&mut avc1_full, *b"avcC", &avcc);

    let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
    write_box(&mut stsd, *b"avc1", &avc1_full);
    write_box(out, *b"stsd", &stsd);
}

fn write_box(out: &mut Vec<u8>, kind: [u8; 4], body: &[u8]) {
    let size = (body.len() + 8) as u32;
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(body);
}

fn write_u32(file: &mut File, value: u32) -> Result<()> {
    file.write_all(&value.to_be_bytes())?;
    Ok(())
}

fn write_u64(file: &mut File, value: u64) -> Result<()> {
    file.write_all(&value.to_be_bytes())?;
    Ok(())
}

fn identity_matrix() -> [u8; 36] {
    let mut matrix = [0u8; 36];
    matrix[0..4].copy_from_slice(&0x00010000u32.to_be_bytes());
    matrix[16..20].copy_from_slice(&0x00010000u32.to_be_bytes());
    matrix[32..36].copy_from_slice(&0x40000000u32.to_be_bytes());
    matrix
}

fn media_duration_and_stts(inner: &Inner) -> (u32, Vec<u8>) {
    let n = inner.samples.len() as u64;
    let elapsed_ms = (inner.started.elapsed().as_millis() as u64).max(n.max(1));
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut prev = 0u64;
    for i in 1..=n {
        let t = elapsed_ms * i / n;
        let delta = (t - prev).max(1) as u32;
        prev += u64::from(delta);
        if let Some((count, last_delta)) = runs.last_mut() {
            if *last_delta == delta {
                *count += 1;
                continue;
            }
        }
        runs.push((1, delta));
    }
    let duration = runs
        .iter()
        .fold(0u32, |acc, (count, delta)| acc.saturating_add(count.saturating_mul(*delta)));
    let mut stts = vec![0, 0, 0, 0];
    stts.extend_from_slice(&(runs.len() as u32).to_be_bytes());
    for (count, delta) in runs {
        stts.extend_from_slice(&count.to_be_bytes());
        stts.extend_from_slice(&delta.to_be_bytes());
    }
    (duration, stts)
}

pub fn split_sps_pps(data: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let nalus = split_nalus(data);
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    for n in nalus {
        if n.is_empty() {
            continue;
        }
        match n[0] & 0x1f {
            7 => sps = n,
            8 => pps = n,
            _ => {}
        }
    }
    (sps, pps)
}

pub fn split_nalus(data: &[u8]) -> Vec<Vec<u8>> {
    if data.starts_with(&[0, 0, 0, 1]) || data.starts_with(&[0, 0, 1]) {
        return split_annexb(data);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= data.len() {
        let n = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        if i + n > data.len() {
            break;
        }
        out.push(data[i..i + n].to_vec());
        i += n;
    }
    if out.is_empty() && !data.is_empty() {
        out.push(data.to_vec());
    }
    out
}

fn split_annexb(data: &[u8]) -> Vec<Vec<u8>> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i..].starts_with(&[0, 0, 0, 1]) {
            starts.push(i + 4);
            i += 4;
        } else if data[i..].starts_with(&[0, 0, 1]) {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut nalus = Vec::new();
    for (idx, start) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).copied().unwrap_or(data.len());
        let mut real_end = end;
        if idx + 1 < starts.len() {
            let prefix = if data[end.saturating_sub(4)..end].starts_with(&[0, 0, 0, 1]) {
                4
            } else {
                3
            };
            real_end = end.saturating_sub(prefix);
        }
        if *start < real_end {
            nalus.push(data[*start..real_end].to_vec());
        }
    }
    nalus
}

pub fn contains_idr(data: &[u8]) -> bool {
    split_nalus(data)
        .iter()
        .any(|nalu| !nalu.is_empty() && nalu[0] & 0x1f == 5)
}

pub fn to_avcc(data: &[u8]) -> Vec<u8> {
    let nalus = split_nalus(data);
    let mut out = Vec::new();
    for n in nalus {
        out.extend_from_slice(&(n.len() as u32).to_be_bytes());
        out.extend_from_slice(&n);
    }
    out
}

pub fn build_avcc(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut avcc = vec![1];
    if sps.len() >= 4 {
        avcc.extend_from_slice(&sps[1..4]);
    } else {
        avcc.extend_from_slice(&[0x64, 0x00, 0x28]);
    }
    avcc.push(0xff);
    avcc.push(0xe1);
    avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(sps);
    avcc.push(1);
    avcc.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(pps);
    avcc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_annex_b_idr() {
        let frame = [0, 0, 0, 1, 0x09, 0x30, 0, 0, 0, 1, 0x65, 0x88, 0x84];
        assert!(contains_idr(&frame));
    }

    #[test]
    fn rejects_non_idr_frame() {
        let frame = [0, 0, 0, 1, 0x09, 0x30, 0, 0, 0, 1, 0x41, 0x9a];
        assert!(!contains_idr(&frame));
    }

    fn temp_mp4() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "atv-recorder-test-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn find_box<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
        find_box_from(data, kind, 0)
    }

    fn find_box_from<'a>(data: &'a [u8], kind: &[u8; 4], start: usize) -> Option<&'a [u8]> {
        let mut i = start;
        while i + 8 <= data.len() {
            let mut size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
            let box_kind = &data[i + 4..i + 8];
            let mut header = 8;
            if size == 1 {
                if i + 16 > data.len() {
                    break;
                }
                size = u64::from_be_bytes(data[i + 8..i + 16].try_into().unwrap()) as usize;
                header = 16;
            } else if size == 0 {
                size = data.len() - i;
            }
            if size < header || i + size > data.len() {
                break;
            }
            let body = &data[i + header..i + size];
            if box_kind == kind {
                return Some(body);
            }
            let nested_start = match box_kind {
                b"avc1" => 78,
                b"stsd" => 8,
                _ => 0,
            };
            if matches!(
                box_kind,
                b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"stsd" | b"avc1"
            ) {
                if let Some(found) = find_box_from(body, kind, nested_start) {
                    return Some(found);
                }
            }
            i += size;
        }
        None
    }

    #[test]
    fn writes_playable_mp4_layout() {
        let path = temp_mp4();
        let recorder = VideoRecorder::new();
        let sps = vec![0x67, 0x64, 0x00, 0x28];
        let pps = vec![0x68, 0xeb, 0xec, 0xb2];
        recorder.start(&path, 1920, 1080, sps, pps).unwrap();
        let frame = vec![0u8; 64];
        recorder.push_sample(&frame, true).unwrap();
        recorder.push_sample(&frame, false).unwrap();
        recorder.finish().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(&bytes[4..8], b"ftyp");
        assert_eq!(&bytes[32..40], &[0, 0, 0, 8, b'f', b'r', b'e', b'e']);
        assert_eq!(&bytes[44..48], b"mdat");
        assert_eq!(&bytes[48..54], &[0u8; 6]);

        let mvhd = find_box(&bytes, b"mvhd").expect("mvhd");
        assert_eq!(mvhd.len(), 100, "mvhd v0 body must be 100 bytes");
        assert_eq!(&mvhd[36..40], &[0x00, 0x01, 0x00, 0x00]);
        assert_eq!(&mvhd[52..56], &[0x00, 0x01, 0x00, 0x00]);
        assert_eq!(&mvhd[68..72], &[0x40, 0x00, 0x00, 0x00]);

        let tkhd = find_box(&bytes, b"tkhd").expect("tkhd");
        assert_eq!(tkhd.len(), 84, "tkhd v0 body must be 84 bytes");

        let avc1 = find_box(&bytes, b"avc1").expect("avc1");
        assert_eq!(&avc1[74..76], &[0x00, 0x18], "depth must be 24");
        assert_eq!(&avc1[76..78], &[0xff, 0xff], "pre_defined must be -1");

        let stsc = find_box(&bytes, b"stsc").expect("stsc");
        assert_eq!(&stsc[12..16], &[0, 0, 0, 1], "one sample per chunk");

        let stco = find_box(&bytes, b"stco").expect("stco");
        assert_eq!(&stco[4..8], &[0, 0, 0, 2], "one chunk offset per sample");
        let first_offset = u32::from_be_bytes(stco[8..12].try_into().unwrap());
        assert_eq!(first_offset, 48);
    }

    #[test]
    fn drops_leading_delta_frames_until_keyframe() {
        let path = temp_mp4();
        let recorder = VideoRecorder::new();
        let sps = vec![0x67, 0x64, 0x00, 0x28];
        let pps = vec![0x68, 0xeb, 0xec, 0xb2];
        recorder.start(&path, 1280, 720, sps, pps).unwrap();
        let frame = vec![0u8; 32];
        recorder.push_sample(&frame, false).unwrap();
        recorder.push_sample(&frame, false).unwrap();
        recorder.push_sample(&frame, true).unwrap();
        recorder.finish().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let stsz = find_box(&bytes, b"stsz").expect("stsz");
        assert_eq!(&stsz[8..12], &[0, 0, 0, 1]);
    }

    #[test]
    fn finish_without_a_keyframe_removes_the_file() {
        let path = temp_mp4();
        let recorder = VideoRecorder::new();
        let sps = vec![0x67, 0x64, 0x00, 0x28];
        let pps = vec![0x68, 0xeb, 0xec, 0xb2];
        recorder.start(&path, 1280, 720, sps, pps).unwrap();
        recorder.push_sample(&[0u8; 16], false).unwrap();
        assert!(recorder.finish().is_err());
        assert!(!path.exists());
    }

    #[test]
    fn split_nalus_does_not_treat_avcc_payload_as_annex_b() {
        let mut avcc = Vec::new();
        avcc.extend_from_slice(&5u32.to_be_bytes());
        avcc.extend_from_slice(&[0x65, 0, 0, 1, 0x42]);
        let nalus = split_nalus(&avcc);
        assert_eq!(nalus, vec![vec![0x65, 0, 0, 1, 0x42]]);
    }
}
