use crate::{ElementTrait, SpatialHack};
use color::rgba_linear;
use derive_setters::Setters;
use stardust_xr_fusion::{
    core::values::{Color, ResourceID},
    drawable::{Line, TextAspect, TextBounds, TextStyle, XAlign, YAlign},
    node::{NodeError, NodeType},
    spatial::{SpatialAspect, Transform},
};

pub trait Transformable: Sized {
    fn transform(&self) -> &Transform;
    fn transform_mut(&mut self) -> &mut Transform;
    fn apply_transform(&self, other: &Self, spatial: &impl SpatialAspect) {
        if self.transform().translation != other.transform().translation
            && self.transform().rotation != other.transform().rotation
            && self.transform().scale != other.transform().scale
        {
            let _ = spatial.set_local_transform(self.transform().clone());
        }
    }

    fn pos(mut self, pos: impl Into<mint::Vector3<f32>>) -> Self {
        self.transform_mut().translation = Some(pos.into());
        self
    }
    fn rot(mut self, rot: impl Into<mint::Quaternion<f32>>) -> Self {
        self.transform_mut().rotation = Some(rot.into());
        self
    }
    fn scl(mut self, scl: impl Into<mint::Vector3<f32>>) -> Self {
        self.transform_mut().scale = Some(scl.into());
        self
    }
}

#[derive(Debug, Setters, Clone)]
#[setters(into)]
pub struct Spatial {
    transform: Transform,
    zoneable: bool,
}
impl ElementTrait for Spatial {
    type Inner = stardust_xr_fusion::spatial::Spatial;
    type Error = NodeError;

    fn create_inner(
        &self,
        spatial_parent: &impl SpatialAspect,
    ) -> Result<Self::Inner, Self::Error> {
        stardust_xr_fusion::spatial::Spatial::create(
            spatial_parent,
            self.transform.clone(),
            self.zoneable,
        )
    }
    fn update_inner(&self, old_decl: &Self, inner: &mut Self::Inner) {
        if self.zoneable != old_decl.zoneable {
            let _ = inner.set_zoneable(self.zoneable);
        }
    }
    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(inner)
    }
}
impl Default for Spatial {
    fn default() -> Self {
        Spatial {
            transform: Transform::none(),
            zoneable: false,
        }
    }
}
impl Transformable for Spatial {
    fn transform(&self) -> &Transform {
        &self.transform
    }
    fn transform_mut(&mut self) -> &mut Transform {
        &mut self.transform
    }
}

pub struct ModelInner {
    model: stardust_xr_fusion::drawable::Model,
    parent: SpatialHack,
}
#[derive(Debug, Clone)]
pub struct Model(Transform, pub ResourceID);
impl ElementTrait for Model {
    type Inner = ModelInner;
    type Error = NodeError;

    fn create_inner(
        &self,
        spatial_parent: &impl SpatialAspect,
    ) -> Result<Self::Inner, Self::Error> {
        Ok(ModelInner {
            model: stardust_xr_fusion::drawable::Model::create(
                spatial_parent,
                self.0.clone(),
                &self.1,
            )?,
            parent: SpatialHack(spatial_parent.node().alias()),
        })
    }

    fn update_inner(&self, old_decl: &Self, inner: &mut Self::Inner) {
        self.apply_transform(old_decl, &inner.model);
        if self.1 != old_decl.1 {
            if let Ok(new_inner) = self.create_inner(&inner.parent) {
                *inner = new_inner;
            }
        }
    }

    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(&inner.model)
    }
}
impl Transformable for Model {
    fn transform(&self) -> &Transform {
        &self.0
    }
    fn transform_mut(&mut self) -> &mut Transform {
        &mut self.0
    }
}
impl Model {
    pub fn namespaced(namespace: &str, path: &str) -> Self {
        Model(
            Transform::none(),
            ResourceID::new_namespaced(namespace, path),
        )
    }
}

#[derive(Debug, Setters, Clone)]
#[setters(into)]
pub struct Lines {
    transform: Transform,
    lines: Vec<Line>,
}
impl ElementTrait for Lines {
    type Inner = stardust_xr_fusion::drawable::Lines;
    type Error = NodeError;

    fn create_inner(&self, parent_space: &impl SpatialAspect) -> Result<Self::Inner, Self::Error> {
        stardust_xr_fusion::drawable::Lines::create(
            parent_space,
            self.transform.clone(),
            &self.lines,
        )
    }

    // figure out why the lines can't be compared
    fn update_inner(&self, old_decl: &Self, inner: &mut Self::Inner) {
        self.apply_transform(old_decl, inner)
        // if self.lines != old_decl.lines {
        //     let _ = inner.set_lines(&self.lines);
        // }
    }

    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(inner)
    }
}
impl Default for Lines {
    fn default() -> Self {
        Lines {
            transform: Transform::none(),
            lines: vec![],
        }
    }
}
impl Transformable for Lines {
    fn transform(&self) -> &Transform {
        &self.transform
    }
    fn transform_mut(&mut self) -> &mut Transform {
        &mut self.transform
    }
}

#[derive(Debug, Setters, Clone)]
#[setters(into)]
pub struct Text {
    transform: Transform,
    text: String,
    character_height: f32,
    color: Color,
    font: Option<ResourceID>,
    text_align_x: XAlign,
    text_align_y: YAlign,
    bounds: Option<TextBounds>,
}
impl ElementTrait for Text {
    type Inner = stardust_xr_fusion::drawable::Text;
    type Error = NodeError;

    fn create_inner(
        &self,
        spatial_parent: &impl SpatialAspect,
    ) -> Result<Self::Inner, Self::Error> {
        stardust_xr_fusion::drawable::Text::create(
            spatial_parent,
            self.transform.clone(),
            &self.text,
            TextStyle {
                character_height: self.character_height,
                color: self.color,
                font: self.font.clone(),
                text_align_x: self.text_align_x,
                text_align_y: self.text_align_y,
                bounds: self.bounds.clone(),
            },
        )
    }
    fn update_inner(&self, old_decl: &Self, inner: &mut Self::Inner) {
        self.apply_transform(old_decl, inner);
        if self.text != old_decl.text {
            let _ = inner.set_text(&self.text);
        }
        if self.character_height != old_decl.character_height {
            let _ = inner.set_character_height(self.character_height);
        }
    }
    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(inner)
    }
}
impl Default for Text {
    fn default() -> Self {
        Text {
            transform: Transform::none(),
            text: "".to_string(),
            character_height: 1.0,
            color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
            font: None,
            text_align_x: XAlign::Left,
            text_align_y: YAlign::Top,
            bounds: None,
        }
    }
}
impl Transformable for Text {
    fn transform(&self) -> &Transform {
        &self.transform
    }
    fn transform_mut(&mut self) -> &mut Transform {
        &mut self.transform
    }
}
