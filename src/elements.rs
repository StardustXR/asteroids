use crate::{ElementTrait, SpatialHack};
use color::rgba_linear;
use derive_setters::Setters;
use mint::Vector2;
use stardust_xr_fusion::{
    core::values::{Color, ResourceID},
    drawable::{Line, TextAspect, TextBounds, TextStyle, XAlign, YAlign},
    node::{NodeError, NodeType},
    spatial::{SpatialAspect, Transform},
};
use stardust_xr_molecules::{DebugSettings, VisualDebug};

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
#[setters(into, strip_option)]
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
    fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
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
#[derive(Debug, Setters, Clone)]
#[setters(into, strip_option)]
pub struct Model {
    transform: Transform,
    pub resource: ResourceID,
}
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
                self.transform.clone(),
                &self.resource,
            )?,
            parent: SpatialHack(spatial_parent.node().alias()),
        })
    }
    fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
        self.apply_transform(old_decl, &inner.model);
        if self.resource != old_decl.resource {
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
        &self.transform
    }
    fn transform_mut(&mut self) -> &mut Transform {
        &mut self.transform
    }
}
impl Model {
    pub fn namespaced(namespace: &str, path: &str) -> Self {
        Model {
            transform: Transform::none(),
            resource: ResourceID::new_namespaced(namespace, path),
        }
    }
}

#[derive(Debug, Setters, Clone)]
#[setters(into, strip_option)]
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
    fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
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
#[setters(into, strip_option)]
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
    fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
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

#[derive(Debug, Setters, Clone)]
#[setters(into, strip_option)]
pub struct Button {
    transform: Transform,
    size: Vector2<f32>,
    max_hover_distance: f32,
    line_thickness: f32,
    accent_color: Color,
    debug: Option<DebugSettings>,
}
impl Default for Button {
    fn default() -> Self {
        Button {
            transform: Transform::none(),
            size: [0.1; 2].into(),
            max_hover_distance: 0.025,
            line_thickness: 0.005,
            accent_color: rgba_linear!(0.0, 1.0, 0.75, 1.0),
            debug: None,
        }
    }
}
impl ElementTrait for Button {
    type Inner = stardust_xr_molecules::button::Button;
    type Error = NodeError;

    fn create_inner(&self, parent_space: &impl SpatialAspect) -> Result<Self::Inner, Self::Error> {
        let mut button = stardust_xr_molecules::button::Button::create(
            parent_space,
            self.transform.clone(),
            self.size,
            stardust_xr_molecules::button::ButtonSettings {
                max_hover_distance: self.max_hover_distance,
                line_thickness: self.line_thickness,
                accent_color: self.accent_color,
            },
        )?;
        button.set_debug(self.debug);
        Ok(button)
    }

    fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
        self.apply_transform(old_decl, inner.touch_plane().root());
        // if self.size != old_decl.size {
        //     inner.touch_plane().set_size(self.size);
        // }
        inner.update();
    }

    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(inner.touch_plane().root())
    }
}
impl Transformable for Button {
    fn transform(&self) -> &Transform {
        &self.transform
    }
    fn transform_mut(&mut self) -> &mut Transform {
        &mut self.transform
    }
}

// #[derive(Debug, Setters, Clone)]
// #[setters(into, strip_option)]
// pub struct Grabbable {
//     transform: Transform,
// }
// impl ElementTrait for Grabbable {
//     type Inner = stardust_xr_molecules::Grabbable;
//     type Error = NodeError;

//     fn create_inner(&self, parent_space: &impl SpatialAspect) -> Result<Self::Inner, Self::Error> {
//         stardust_xr_molecules::Grabbable::create(
//             parent_space,
//             self.transform.clone(),
//             field,
//             GrabbableSettings::default(),
//         )
//     }

//     fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
//         self.apply_transform(old_decl, inner.content_parent())
//     }

//     fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
//         Some(inner.content_parent())
//     }
// }
// impl Transformable for Grabbable {
//     fn transform(&self) -> &Transform {
//         &self.transform
//     }
//     fn transform_mut(&mut self) -> &mut Transform {
//         &mut self.transform
//     }
// }
