pub(crate) struct FontTextures {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    fallback_textures: [wgpu::Texture; 2],
    pub dirty: bool,
}

impl FontTextures {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("egui_font_textures"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let textures = [
            ("egui_font_curves_empty", wgpu::TextureFormat::Rgba32Float),
            ("egui_font_bands_empty", wgpu::TextureFormat::Rgba32Uint),
        ]
        .map(|(label, format)| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        let bind_group = Self::bind(device, &layout, textures.each_ref());
        Self {
            layout,
            bind_group,
            fallback_textures: textures,
            dirty: false,
        }
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        textures: [&wgpu::Texture; 2],
    ) -> wgpu::BindGroup {
        let views = textures.map(|texture| texture.create_view(&Default::default()));
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("egui_font_textures"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
            ],
        })
    }

    pub fn update_bind_group(
        &mut self,
        device: &wgpu::Device,
        textures: &ahash::HashMap<epaint::TextureId, crate::renderer::Texture>,
    ) {
        if !self.dirty {
            return;
        }
        let textures = [epaint::TextureId::FontCurves, epaint::TextureId::FontBands]
            .map(|id| textures.get(&id)?.texture.as_ref());
        self.bind_group = Self::bind(
            device,
            &self.layout,
            [
                textures[0].unwrap_or(&self.fallback_textures[0]),
                textures[1].unwrap_or(&self.fallback_textures[1]),
            ],
        );
        self.dirty = false;
    }
}
