use std::process::*;
use std::io::{
	self,
	Read,
	Write,
};

#[derive(serde::Deserialize, Debug)]
struct Meta {
	streams: Vec<Stream>,
}
#[derive(serde::Deserialize, Debug)]
struct Stream {
	codec_type: String,
	width: Option<usize>,
	height: Option<usize>,
}

fn main() {
	let mut args = std::env::args();
	let _ = args.next();
	let fname = args.next().expect("missing argument");

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
	let chunk_size = stream.width.expect("missing width") * stream.height.expect("missing height") * 3; // x3 for each channel

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
	let mut histograms: Vec<Vec<u32>> = vec![vec![0; 256]; chunk_size];
	//let ffmpeg = BufReader::with_capacity(chunk_size, ffmpeg);
	loop {
		if ffmpeg.read_exact(&mut frame).is_err() {
			// TODO distinguish UnexpectedEof with 0 bytes read, other UnexpectedEofs, other errors
			break;
		}
		for i in 0..chunk_size {
			histograms[i][ frame[i] as usize ] += 1;
		}
	}
}
