use twilight_model::gateway::payload::outgoing::identify::IdentifyProperties;

#[derive(Clone, Copy, Debug)]
pub struct Android<'a> {
    version: &'a str,
    build_number: &'a str,
    device: &'a str,
    os_sdk_version: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Linux<'a> {
    os_version: &'a str,
}

#[inline]
pub fn android<'a>() -> Android<'a> {
    // 115.4 - Beta on a Pixel running Android 12 (from https://github.com/MateriApps/OpenCord).
    Android {
        version: "115.4 - Beta",
        build_number: "114104",
        device: "Pixel, coral",
        os_sdk_version: 31,
    }
}

#[inline]
pub fn linux<'a>() -> Linux<'a> {
    // Linux stable. (from https://kernel.org/)
    Linux {
        os_version: "5.18.5",
    }
}

impl<'a> Android<'a> {
    #[inline]
    pub fn finish(self) -> IdentifyProperties {
        IdentifyProperties::default().android(
            self.version,
            self.build_number,
            self.device,
            self.os_sdk_version,
        )
    }
}

impl<'a> Linux<'a> {
    #[inline]
    pub fn finish(self) -> IdentifyProperties {
        IdentifyProperties::default().linux(self.os_version)
    }
}
