use std::process::*;
use std::io::{
	self,
	Read,
	Write,
	BufWriter,
};
use rayon::prelude::*;
use std::collections::VecDeque;

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
	let mut histograms: Vec<VecDeque<u32>> = vec![VecDeque::from([0; 256]); chunk_size];
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
				hist[val as usize] += 1;
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
				while hist.get(0) == Some(&0) {
					hist.pop_front();
				}
				if hist.is_empty() {
					// collect() into `frame = None`, signalling that we drained the histogram
					return None;
				}
				hist[0] -= 1;
				// len < 256 means 0th element is actually (256-len)th one
				// because we already checked previous (256-len-1)
				Some((256 - hist.len()) as u8)
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
