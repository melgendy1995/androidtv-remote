use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use crate::error::{AppError, Result};

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
        let mut file = File::create(path)?;
        write_ftyp(&mut file)?;
        // Reserve the mdat header up front; patched in finalize(). Always
        // written in largesize form (size=1 + u64) so recordings beyond
        // 4 GB stay valid MP4s.
        file.write_all(&[0u8; 16])?;
        let mdat_start = file.stream_position()? - 16;
        let inner = Inner {
            path: path.to_string_lossy().to_string(),
            file,
            mdat_start,
            samples: Vec::new(),
            sps,
            pps,
            width: width.max(16),
            height: height.max(16),
            started: Instant::now(),
        };
        *self.inner.lock().unwrap() = Some(inner);
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
        finalize(&mut inner)?;
        Ok(inner.path)
    }
}

fn finalize(inner: &mut Inner) -> Result<()> {
    let mdat_end = inner.file.stream_position()?;
    let mdat_size = mdat_end - inner.mdat_start;
    inner.file.seek(SeekFrom::Start(inner.mdat_start))?;
    write_u32(&mut inner.file, 1)?;
    inner.file.write_all(b"mdat")?;
    write_u64(&mut inner.file, mdat_size)?;
    inner.file.seek(SeekFrom::End(0))?;

    let mut moov = Vec::new();
    write_moov(&mut moov, inner)?;
    inner.file.write_all(&moov)?;
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
    let mut moov = Vec::new();
    write_mvhd(&mut moov, inner.samples.len() as u32);
    write_trak(&mut moov, inner)?;
    write_box(out, *b"moov", &moov);
    Ok(())
}

fn write_mvhd(out: &mut Vec<u8>, frames: u32) {
    let mut body = Vec::new();
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&30u32.to_be_bytes());
    body.extend_from_slice(&frames.to_be_bytes());
    body.extend_from_slice(&0x00010000u32.to_be_bytes());
    body.extend_from_slice(&0x0100u16.to_be_bytes());
    body.extend_from_slice(&0u16.to_be_bytes());
    body.extend_from_slice(&0u64.to_be_bytes());
    body.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0x00, 0x00]);
    body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend_from_slice(&[0x40, 0x00, 0x00, 0x00]);
    body.extend_from_slice(&[0u8; 24]);
    body.extend_from_slice(&2u32.to_be_bytes());
    write_box(out, *b"mvhd", &body);
}

fn write_trak(out: &mut Vec<u8>, inner: &Inner) -> Result<()> {
    let mut trak = Vec::new();
    write_tkhd(&mut trak, inner);
    write_mdia(&mut trak, inner)?;
    write_box(out, *b"trak", &trak);
    Ok(())
}

fn write_tkhd(out: &mut Vec<u8>, inner: &Inner) {
    let mut body = Vec::new();
    body.extend_from_slice(&0x00000007u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&1u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
    body.extend_from_slice(&0u64.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0x00, 0x00]);
    body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend_from_slice(&[0x40, 0x00, 0x00, 0x00]);
    body.extend_from_slice(&(inner.width << 16).to_be_bytes());
    body.extend_from_slice(&(inner.height << 16).to_be_bytes());
    write_box(out, *b"tkhd", &body);
}

fn write_mdia(out: &mut Vec<u8>, inner: &Inner) -> Result<()> {
    let mut mdia = Vec::new();
    let mut mdhd = Vec::new();
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&30u32.to_be_bytes());
    mdhd.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
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
    write_stbl(&mut minf, inner)?;
    write_box(&mut mdia, *b"minf", &minf);
    write_box(out, *b"mdia", &mdia);
    Ok(())
}

fn write_stbl(out: &mut Vec<u8>, inner: &Inner) -> Result<()> {
    let mut stbl = Vec::new();
    write_stsd(&mut stbl, inner);
    let mut stts = vec![0, 0, 0, 0, 0, 0, 0, 1];
    stts.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
    stts.extend_from_slice(&1u32.to_be_bytes());
    write_box(&mut stbl, *b"stts", &stts);

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

    let mut stsc = vec![0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1];
    stsc.extend_from_slice(&(inner.samples.len() as u32).to_be_bytes());
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
    avc1[41] = 1;
    avc1[72..74].copy_from_slice(&0x0018u16.to_be_bytes());
    avc1[74..76].copy_from_slice(&(-1i16).to_be_bytes());
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
    if data.windows(4).any(|w| w == [0, 0, 0, 1]) || data.windows(3).any(|w| w == [0, 0, 1]) {
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

    #[test]
    fn writes_ftyp_largesize_mdat_and_valid_offsets() {
        let path = std::env::temp_dir().join(format!(
            "atv-recorder-test-{}.mp4",
            std::process::id()
        ));
        let recorder = VideoRecorder::new();
        let sps = vec![0x67, 0x64, 0x00, 0x28];
        let pps = vec![0x68, 0xeb, 0xec, 0xb2];
        recorder.start(&path, 1920, 1080, sps.clone(), pps.clone()).unwrap();
        let frame = vec![0u8; 64];
        recorder.push_sample(&frame, true).unwrap();
        recorder.push_sample(&frame, false).unwrap();
        recorder.finish().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        // ftyp first (32 bytes: size+type+isom+minor+4 brands)
        assert_eq!(&bytes[4..8], b"ftyp");
        assert_eq!(&bytes[32..36], &[0, 0, 0, 1]); // largesize marker
        assert_eq!(&bytes[36..40], b"mdat");
        let mdat_size = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
        // first sample data starts right after the 16-byte mdat header
        assert_eq!(&bytes[48..54], &[0u8; 6]);
        // moov appended after mdat
        let moov_at = 32 + mdat_size as usize;
        assert_eq!(&bytes[moov_at + 4..moov_at + 8], b"moov");
        // sample offsets point at real data (first frame right after mdat header)
        assert!(bytes.windows(4).any(|w| w == b"stco"));
    }
}
