use kestrel_proto::KestrelError;
use muxide::api::{AacProfile, AudioCodec, MuxerBuilder, VideoCodec};
use std::fs::File;
use std::path::Path;

pub struct MuxOptions {
    pub video_path: Option<String>,
    pub audio_path: Option<String>,
    pub output_path: String,
    pub video_width: Option<u32>,
    pub video_height: Option<u32>,
    pub fps: Option<f64>,
}

/// Pure-Rust MP4 muxer using `muxide`
pub async fn mux_to_mp4(options: MuxOptions) -> Result<(), KestrelError> {
    tokio::task::spawn_blocking(move || {
        mux_to_mp4_blocking(options)
    }).await.map_err(|e| KestrelError::Muxer(e.to_string()))?
}

fn mux_to_mp4_blocking(options: MuxOptions) -> Result<(), KestrelError> {
    if options.video_path.is_none() && options.audio_path.is_none() {
        return Err(KestrelError::Muxer("No input streams provided for muxing".into()));
    }

    if let (Some(vpath), Some(apath)) = (options.video_path.as_ref(), options.audio_path.as_ref()) {
        let out_file = File::create(&options.output_path)
            .map_err(|e| KestrelError::Io(format!("Failed to create output file {}: {}", options.output_path, e)))?;

        let width = options.video_width.unwrap_or(1920);
        let height = options.video_height.unwrap_or(1080);
        let fps = options.fps.unwrap_or(30.0);

        let builder = MuxerBuilder::new(out_file)
            .video(VideoCodec::H264, width, height, fps)
            .audio(AudioCodec::Aac(AacProfile::Lc), 44100, 2);

        let mut muxer = builder.build()
            .map_err(|e| KestrelError::Muxer(format!("Failed to build muxer: {:?}", e)))?;

        // Read elementary streams and feed packets
        let vbytes = std::fs::read(vpath)
            .map_err(|e| KestrelError::Io(format!("Failed to read video part: {}", e)))?;
        let abytes = std::fs::read(apath)
            .map_err(|e| KestrelError::Io(format!("Failed to read audio part: {}", e)))?;

        // Write video and audio frames if available
        if !vbytes.is_empty() {
            let _ = muxer.write_video(0.0, &vbytes, true);
        }
        if !abytes.is_empty() {
            let _ = muxer.write_audio(0.0, &abytes);
        }

        muxer.finish()
            .map_err(|e| KestrelError::Muxer(format!("Failed to finish muxing: {:?}", e)))?;

        // Clean up temporary elementary track files
        let _ = std::fs::remove_file(vpath);
        let _ = std::fs::remove_file(apath);
    } else if let Some(vpath) = options.video_path.as_ref() {
        if Path::new(vpath) != Path::new(&options.output_path) {
            std::fs::rename(vpath, &options.output_path)
                .or_else(|_| std::fs::copy(vpath, &options.output_path).map(|_| ()))
                .map_err(|e| KestrelError::Io(e.to_string()))?;
        }
    } else if let Some(apath) = options.audio_path.as_ref() {
        if Path::new(apath) != Path::new(&options.output_path) {
            std::fs::rename(apath, &options.output_path)
                .or_else(|_| std::fs::copy(apath, &options.output_path).map(|_| ()))
                .map_err(|e| KestrelError::Io(e.to_string()))?;
        }
    }

    Ok(())
}
