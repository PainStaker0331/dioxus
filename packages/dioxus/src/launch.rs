//! Launch helper macros for fullstack apps
#![allow(unused)]
use std::any::Any;

use crate::prelude::*;
use dioxus_core::prelude::*;
use dioxus_core::{BoxedContext, CrossPlatformConfig, PlatformBuilder};

/// A builder for a fullstack app.
pub struct LaunchBuilder<Platform: PlatformBuilder = CurrentPlatform> {
    cross_platform_config: CrossPlatformConfig,
    platform_config: Option<<Platform as PlatformBuilder>::Config>,
}

// Default platform builder
impl LaunchBuilder {
    /// Create a new builder for your application. This will create a launch configuration for the current platform based on the features enabled on the `dioxus` crate.
    pub fn new<Props: Clone + Default + 'static, M: 'static>(
        component: impl ComponentFunction<Props, M>,
    ) -> Self {
        Self {
            cross_platform_config: CrossPlatformConfig::new(
                component,
                Default::default(),
                Default::default(),
            ),
            platform_config: None,
        }
    }

    /// Create a new builder for your application with some root props. This will create a launch configuration for the current platform based on the features enabled on the `dioxus` crate.
    pub fn new_with_props<Props: Clone + 'static, M: 'static>(
        component: impl ComponentFunction<Props, M>,
        props: Props,
    ) -> Self {
        Self {
            cross_platform_config: CrossPlatformConfig::new(component, props, Default::default()),
            platform_config: None,
        }
    }
}

impl<Platform: PlatformBuilder> LaunchBuilder<Platform> {
    /// Inject state into the root component's context.
    pub fn context(mut self, state: impl Any + Clone + 'static) -> Self {
        self.cross_platform_config
            .push_context(BoxedContext::new(state));
        self
    }

    /// Provide a platform-specific config to the builder.
    pub fn cfg(mut self, config: impl Into<Option<<Platform as PlatformBuilder>::Config>>) -> Self {
        if let Some(config) = config.into() {
            self.platform_config = Some(config);
        }
        self
    }

    #[allow(clippy::unit_arg)]
    /// Launch the app.
    pub fn launch(self) {
        Platform::launch(
            self.cross_platform_config,
            self.platform_config.unwrap_or_default(),
        );
    }
}

#[cfg(feature = "web")]
impl LaunchBuilder<dioxus_web::WebPlatform> {
    /// Launch your web application.
    pub fn launch_web(self) {
        dioxus_web::WebPlatform::launch(
            self.cross_platform_config,
            self.platform_config.unwrap_or_default(),
        );
    }
}

#[cfg(feature = "desktop")]
impl LaunchBuilder<dioxus_desktop::DesktopPlatform> {
    /// Launch your desktop application.
    pub fn launch_desktop(self) {
        dioxus_desktop::DesktopPlatform::launch(
            self.cross_platform_config,
            self.platform_config.unwrap_or_default(),
        );
    }
}

#[cfg(feature = "desktop")]
type CurrentPlatform = dioxus_desktop::DesktopPlatform;
#[cfg(all(feature = "web", not(feature = "desktop")))]
type CurrentPlatform = dioxus_web::WebPlatform;
#[cfg(not(any(feature = "desktop", feature = "web")))]
type CurrentPlatform = ();

/// Launch your application without any additional configuration. See [`LaunchBuilder`] for more options.
pub fn launch<Props, Marker: 'static>(component: impl ComponentFunction<Props, Marker>)
where
    Props: Default + Clone + 'static,
{
    LaunchBuilder::new(component).launch()
}

#[cfg(feature = "web")]
/// Launch your web application without any additional configuration. See [`LaunchBuilder`] for more options.
pub fn launch_web<Props, Marker: 'static>(component: impl ComponentFunction<Props, Marker>)
where
    Props: Default + Clone + 'static,
{
    LaunchBuilder::new(component).launch_web()
}

#[cfg(feature = "desktop")]
/// Launch your desktop application without any additional configuration. See [`LaunchBuilder`] for more options.
pub fn launch_desktop<Props, Marker: 'static>(component: impl ComponentFunction<Props, Marker>)
where
    Props: Default + Clone + 'static,
{
    LaunchBuilder::new(component).launch_desktop()
}
