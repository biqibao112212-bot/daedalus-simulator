use crate::robomaster::prelude::{ArmorLabel, ArmorType};
use bevy::prelude::*;
use image::ExtendedColorType::Rgb8;
use image::codecs::jpeg::JpegEncoder;
use std::fs::{File, create_dir_all};
use std::io::ErrorKind::Other;
use std::io::{BufWriter, Error, Write};
use std::path::{Path, PathBuf};

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum ArmorColor {
    Blue = 0,
    Red = 1,
    Gray = 2,
    Purple = 3,
}

#[derive(Debug, Clone)]
pub struct ArmorEntry {
    pub color: ArmorColor,
    pub typ: ArmorType,
    pub label: ArmorLabel,
    pub points: [Vec2; 4],
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BuffPoseClass {
    RedUnlit = 0,
    RedPending = 1,
    RedActivated = 2,
    BlueUnlit = 3,
    BluePending = 4,
    BlueActivated = 5,
}

#[derive(Debug, Clone)]
pub struct BuffPoseEntry {
    pub class_id: BuffPoseClass,
    pub bbox: Vec4,
    pub keypoints: [Vec2; 5],
}

pub struct DatasetWriter {
    image_dir: PathBuf,
    label_dir: PathBuf,
    depth_dir: PathBuf,
    buff_image_dir: PathBuf,
    buff_label_dir: PathBuf,
    seq: u64,
}

impl DatasetWriter {
    pub fn new(directory: &str) -> std::io::Result<Self> {
        let base = Path::new(directory);
        let image_dir = base.join("images");
        let label_dir = base.join("label");
        let depth_dir = base.join("depth");
        let buff_base = base.join("buff");
        let buff_image_dir = buff_base.join("images");
        let buff_label_dir = buff_base.join("labels");

        create_dir_all(&image_dir)?;
        create_dir_all(&label_dir)?;
        create_dir_all(&depth_dir)?;
        create_dir_all(&buff_image_dir)?;
        create_dir_all(&buff_label_dir)?;
        write_buff_data_yaml(&buff_base)?;

        Ok(Self {
            image_dir,
            label_dir,
            depth_dir,
            buff_image_dir,
            buff_label_dir,
            seq: 0,
        })
    }

    pub fn next_frame_name(&mut self) -> String {
        self.seq += 1;
        format!("frame_{:06}", self.seq)
    }

    pub fn write_color_entry(
        &mut self,
        frame: &str,
        height: u32,
        width: u32,
        data: &[u8],
        entries: &[ArmorEntry],
    ) -> std::io::Result<()> {
        self.save_image(
            height,
            width,
            data,
            &self.image_dir.join(format!("{}.jpg", frame)),
        )?;
        let mut writer =
            BufWriter::new(File::create(self.label_dir.join(format!("{}.txt", frame)))?);

        for entry in entries {
            write!(
                writer,
                "{} {} {}",
                entry.color as u8, entry.typ as u8, entry.label as u8
            )?;
            for p in &entry.points {
                write!(writer, " {:.6} {:.6}", p.x, p.y)?;
            }
            writeln!(writer)?;
        }

        writer.flush()?;
        Ok(())
    }

    pub fn write_buff_pose_entry(
        &mut self,
        frame: &str,
        height: u32,
        width: u32,
        data: &[u8],
        entries: &[BuffPoseEntry],
    ) -> std::io::Result<()> {
        self.save_image(
            height,
            width,
            data,
            &self.buff_image_dir.join(format!("{}.jpg", frame)),
        )?;
        let mut writer = BufWriter::new(File::create(
            self.buff_label_dir.join(format!("{}.txt", frame)),
        )?);

        for entry in entries {
            write!(
                writer,
                "{} {:.6} {:.6} {:.6} {:.6}",
                entry.class_id as u8, entry.bbox.x, entry.bbox.y, entry.bbox.z, entry.bbox.w
            )?;
            for point in &entry.keypoints {
                write!(writer, " {:.6} {:.6}", point.x, point.y)?;
            }
            writeln!(writer)?;
        }

        writer.flush()?;
        Ok(())
    }

    pub fn write_depth_entry(
        &mut self,
        frame: &str,
        width: u32,
        height: u32,
        depth_bytes: &[u8],
        near: f32,
        far: f32,
    ) -> std::io::Result<()> {
        let depth_mm = depth_bytes_to_mm(depth_bytes, near, far);
        let mut bytes = Vec::with_capacity(depth_mm.len() * 2);
        for value in depth_mm {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        image::save_buffer(
            self.depth_dir.join(format!("{}.png", frame)),
            bytes.as_slice(),
            width,
            height,
            image::ColorType::L16,
        )
        .map_err(|e| Error::new(Other, e))?;
        Ok(())
    }

    fn save_image(&self, height: u32, width: u32, data: &[u8], path: &Path) -> std::io::Result<()> {
        JpegEncoder::new(&mut File::create(path)?)
            .encode(data, width, height, Rgb8)
            .map_err(|e| Error::new(Other, e))?;
        Ok(())
    }
}

fn write_buff_data_yaml(base: &Path) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(base.join("data.yaml"))?);
    writeln!(writer, "path: .")?;
    writeln!(writer, "train: images")?;
    writeln!(writer, "val: images")?;
    writeln!(writer, "kpt_shape: [5, 2]")?;
    writeln!(writer, "names:")?;
    writeln!(writer, "  0: R-Unlit")?;
    writeln!(writer, "  1: R-Pending")?;
    writeln!(writer, "  2: R-Activated")?;
    writeln!(writer, "  3: B-Unlit")?;
    writeln!(writer, "  4: B-Pending")?;
    writeln!(writer, "  5: B-Activated")?;
    writer.flush()
}

fn depth_bytes_to_mm(data: &[u8], near: f32, far: f32) -> Vec<u16> {
    data.chunks_exact(4)
        .map(|chunk| {
            let depth = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let meters = if depth <= f32::EPSILON {
                far
            } else {
                (near / depth).clamp(near, far)
            };
            (meters * 1000.0).round().clamp(0.0, u16::MAX as f32) as u16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_z_depth_converts_to_mm() {
        let raw = (0.1f32).to_le_bytes();
        let got = depth_bytes_to_mm(&raw, 0.1, 80.0);
        assert_eq!(got[0], 1000);
    }

    #[test]
    fn buff_pose_writer_uses_yolo_pose_format() {
        let base =
            std::env::temp_dir().join(format!("daedalus_buff_pose_writer_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut writer = DatasetWriter::new(base.to_str().unwrap()).unwrap();
        let frame = writer.next_frame_name();
        let data = [128u8, 128, 128];
        let entry = BuffPoseEntry {
            class_id: BuffPoseClass::RedPending,
            bbox: Vec4::new(0.5, 0.5, 0.25, 0.125),
            keypoints: [
                Vec2::new(0.4, 0.4),
                Vec2::new(0.6, 0.4),
                Vec2::new(0.6, 0.6),
                Vec2::new(0.4, 0.6),
                Vec2::new(0.5, 0.5),
            ],
        };

        writer
            .write_buff_pose_entry(&frame, 1, 1, &data, &[entry])
            .unwrap();

        let label = std::fs::read_to_string(
            base.join("buff")
                .join("labels")
                .join(format!("{}.txt", frame)),
        )
        .unwrap();
        assert!(label.starts_with("1 0.500000 0.500000 0.250000 0.125000"));
        assert_eq!(label.split_whitespace().count(), 15);
        assert!(base.join("buff").join("data.yaml").exists());
        let _ = std::fs::remove_dir_all(base);
    }
}
