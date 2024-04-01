use crate::{ElementTrait, SpatialHack};
use derive_setters::Setters;
use stardust_xr_fusion::{
    core::values::ResourceID,
    drawable::Line,
    node::{NodeError, NodeType},
    spatial::{SpatialAspect, Transform},
};

#[derive(Debug, Setters, Clone)]
#[setters(into)]
pub struct Spatial {
    pub pos: mint::Vector3<f32>,
    pub rot: mint::Quaternion<f32>,
    pub scl: mint::Vector3<f32>,
    pub zoneable: bool,
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
            Transform::from_translation(self.pos),
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
            pos: [0.0; 3].into(),
            rot: [0.0, 0.0, 0.0, 1.0].into(),
            scl: [1.0; 3].into(),
            zoneable: false,
        }
    }
}

pub struct ModelInner {
    model: stardust_xr_fusion::drawable::Model,
    parent: SpatialHack,
}
#[derive(Debug, Clone)]
pub struct Model(pub ResourceID);
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
                Transform::identity(),
                &self.0,
            )?,
            parent: SpatialHack(spatial_parent.node().alias()),
        })
    }

    fn update_inner(&self, old_decl: &Self, inner: &mut Self::Inner) {
        if self.0 != old_decl.0 {
            if let Ok(new_inner) = self.create_inner(&inner.parent) {
                *inner = new_inner;
            }
        }
    }

    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(&inner.model)
    }
}

#[derive(Debug, Clone)]
pub struct Lines(pub Vec<Line>);
impl ElementTrait for Lines {
    type Inner = stardust_xr_fusion::drawable::Lines;
    type Error = NodeError;

    fn create_inner(&self, parent_space: &impl SpatialAspect) -> Result<Self::Inner, Self::Error> {
        stardust_xr_fusion::drawable::Lines::create(parent_space, Transform::identity(), &self.0)
    }

    // figure out why the lines can't be compared
    fn update_inner(&self, old_decl: &Self, inner: &mut Self::Inner) {
        // if self.lines != old_decl.lines {
        //     let _ = inner.set_lines(&self.lines);
        // }
    }

    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(inner)
    }
}
