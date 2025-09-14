use std::process::*;
use std::io::{
	self,
	Read,
	Write,
	BufWriter,
};
use rayon::prelude::*;
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Debug)]
struct Meta {
	streams: Vec<Stream>,
}
#[derive(serde::Deserialize, Debug)]
struct Stream {
	codec_type: String,
	width: Option<usize>,
	height: Option<usize>,
	r_frame_rate: Option<String>,
}

fn main() {
	let mut args = std::env::args();
	let _ = args.next();
	let fname = args.next().expect("missing argument: fname");
	let outname = args.next().expect("missing argument: outname");

	let ffprobe = Command::new("ffprobe")
		.args([
			"-v", "error",
			"-print_format", "json",
			"-show_streams",
			&fname,
		])
		.output()
		.expect("failed to run ffprobe");
	io::stderr().write_all(&ffprobe.stderr).unwrap();
	io::stderr().flush().unwrap();
	if ! ffprobe.status.success() {
		panic!("ffprobe is not happy");
	}
	let meta: Meta = serde_json::from_slice(&ffprobe.stdout).expect("invalid json from ffprobe");
	let mut stream: Vec<_> = meta.streams.into_iter().filter(|s| s.codec_type == "video").collect();
	if stream.len() != 1 {
		panic!("expected exactly one video stream, found {}", stream.len());
	}
	let stream = stream.pop().unwrap();
	let width = stream.width.expect("missing width");
	let height = stream.height.expect("missing height");
	let chunk_size = width * height * 3; // x3 for each channel
	let framerate = stream.r_frame_rate.expect("missing framerate");

	let ffmpeg = Command::new("ffmpeg")
		.args([
			"-i", &fname,
			"-v", "error",
			"-pix_fmt", "rgb24",
			"-vcodec", "rawvideo",
			"-f", "image2pipe",
			"-",
		])
		.stdout(Stdio::piped())
		.spawn()
		.expect("failed to run ffmpeg");
	let mut ffmpeg = ffmpeg.stdout.expect("missing ffmpeg stdout o_O");
	let mut frame = vec![0; chunk_size];
	// 2^16 frames at 30 FPS is 36:24 and a change
	// 2^32 frames at 60 FPS is over 828 days, that should be enough
	let mut histograms: Vec<BTreeMap<u8, u32>> = vec![BTreeMap::new(); chunk_size];
	//let ffmpeg = BufReader::with_capacity(chunk_size, ffmpeg);
	loop {
		if ffmpeg.read_exact(&mut frame).is_err() {
			// TODO distinguish UnexpectedEof with 0 bytes read, other UnexpectedEofs, other errors
			break;
		}
		let hchunks: Vec<_> = histograms.chunks_mut(131072).collect();
		let fchunks: Vec<_> = frame.chunks(131072).collect();
		hchunks.into_par_iter()
			.zip(fchunks.into_par_iter())
			.for_each(|(hc, fc)| {
				for (hist, &val) in hc.iter_mut().zip(fc.iter()) {
					*hist.entry(val).or_default() += 1;
				}
			});
	}

	let mut ffmpeg = Command::new("ffmpeg")
		.args([
			"-y", // XXX should probably let user decide whether to err out on existing file or overwrite it
			"-f", "rawvideo",
			"-pix_fmt", "rgb24",
			"-framerate", &framerate,
			"-s", &format!("{width}x{height}"),
			"-i", "-",
			"-c:v", "libx264",
			&outname,
		])
		.stdin(Stdio::piped())
		.spawn()
		.expect("failed to run ffmpeg");
	let input = ffmpeg.stdin.as_mut().expect("missing ffmpeg stdin o_O");
	let mut input = BufWriter::new(input);
	loop {
		let frame: Option<Vec<_>> =
			histograms.par_iter_mut()
			.map(|hist| {
				if let Some((&k, &v)) = hist.first_key_value() {
					if v == 0 {
						hist.remove(&k);
					}
				}
				for (val, count) in hist.iter_mut() {
					*count -= 1;
					return Some(*val);
				}
				None // collect() into `frame = None`, signalling that we drained the histogram
			})
			.collect();
		match frame {
			Some(f) => {
				input.write(f.as_slice()).expect("failed to feed ffmpeg");
			},
			None => break,
		}
	}
	// this will flush BufWriter
	let _ = input.into_inner().unwrap();
	// this will flush raw stdin
	ffmpeg.wait().unwrap();
}
