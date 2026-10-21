use crate::{
    ImageData, ImageDelta, TextureId,
    textures::{TextureOptions, TexturesDelta},
};
use alloc::sync::Arc;

pub(super) struct Texture<T> {
    pub data: Vec<[T; 4]>,
    width: usize,
    max_height: usize,
    height: usize,
    uploaded: usize,
}

impl<T: Copy + Default> Texture<T> {
    pub fn new(width: usize, max_height: usize) -> Self {
        Self {
            data: Vec::new(),
            width,
            max_height,
            height: 0,
            uploaded: 0,
        }
    }

    pub fn take_delta(
        &mut self,
        delta: &mut TexturesDelta,
        id: TextureId,
        image: impl Fn([usize; 2], Arc<[[T; 4]]>) -> ImageData,
    ) {
        let height = self
            .data
            .len()
            .div_ceil(self.width)
            .max(1)
            .next_power_of_two()
            .min(self.max_height);
        if height == self.height {
            let mut start = self.uploaded;
            while start < self.data.len() {
                let x = start % self.width;
                let remaining = self.data.len() - start;
                let width = (self.width - x).min(remaining);
                let rows = if x == 0 && width == self.width {
                    remaining / self.width
                } else {
                    1
                };
                let end = start + width * rows;
                delta.push(
                    id,
                    ImageDelta::partial(
                        [x, start / self.width],
                        image([width, rows], self.data[start..end].into()),
                        TextureOptions::NEAREST,
                    ),
                );
                start = end;
            }
        } else {
            let mut pixels = self.data.clone();
            pixels.resize(self.width * height, [T::default(); 4]);
            delta.push(
                id,
                ImageDelta::full(
                    image([self.width, height], pixels.into()),
                    TextureOptions::NEAREST,
                ),
            );
        }
        self.uploaded = self.data.len();
        self.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_updates_survive_growth_and_reallocation() {
        let id = TextureId::FontBands;
        let image = |resolution, pixels| ImageData::Rgba32Uint { resolution, pixels };
        let mut texture = Texture::new(4, 8);
        let mut pending = TexturesDelta::default();
        for count in [3, 1, 1, 2, 2, 6] {
            let start = texture.data.len() as u32;
            texture
                .data
                .extend((start..start + count).map(|value| [value; 4]));
            let mut delta = TexturesDelta::default();
            texture.take_delta(&mut delta, id, image);
            pending.append(delta);
        }
        let mut pixels = Vec::new();
        let mut stride = 0;
        for update in pending.set.remove(&id).unwrap() {
            let ImageData::Rgba32Uint {
                resolution,
                pixels: data,
            } = update.image
            else {
                unreachable!()
            };
            if let Some([x, y]) = update.pos {
                for row in 0..resolution[1] {
                    let start = (y + row) * stride + x;
                    pixels[start..start + resolution[0]]
                        .copy_from_slice(&data[row * resolution[0]..(row + 1) * resolution[0]]);
                }
            } else {
                stride = resolution[0];
                pixels = data.to_vec();
            }
        }
        assert_eq!(
            &pixels[..texture.data.len()],
            texture.data,
            "Queued patches corrupted texture contents"
        );
        texture.take_delta(&mut pending, id, image);
        assert!(pending.is_empty(), "Unchanged texture emitted an update");

        pending.free(id);
        let mut replacement = Texture::new(4, 8);
        replacement.data.push([99; 4]);
        let mut delta = TexturesDelta::default();
        replacement.take_delta(&mut delta, id, image);
        pending.append(delta);
        assert!(
            !pending.free.contains(&id),
            "An earlier free would destroy the replacement texture"
        );
        assert!(
            pending.set[&id][0].is_whole(),
            "Reallocated texture needs a full upload"
        );
        let mut release = TexturesDelta::default();
        release.free(id);
        pending.append(release);
        assert!(
            pending.set.is_empty(),
            "Released texture retained pending uploads"
        );
        assert!(pending.free.contains(&id), "Release was lost");
        pending.clear();
    }
}
