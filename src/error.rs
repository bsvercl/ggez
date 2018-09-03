//! Error types and conversion functions.

use std::error::Error;
use std::fmt;
use std::io;
use std::path;

use ash;
use ash::vk;
use winit;

use app_dirs2::AppDirsError;
use gilrs;
use image;
use rodio::decoder::DecoderError;
use toml;
use zip;

/// An enum containing all kinds of game framework errors.
#[derive(Debug)]
pub enum GameError {
    /// An error in the filesystem layout
    FilesystemError(String),
    /// An error in the config file
    ConfigError(String),
    /// Happens when an `EventsLoopProxy` attempts to
    /// wake up an `EventsLoop` that no longer exists.
    EventLoopError(String),
    /// An error trying to load a resource, such as getting an invalid image file.
    ResourceLoadError(String),
    /// Unable to find a resource; the Vec is the paths it searched for and associated errors
    ResourceNotFound(String, Vec<(path::PathBuf, GameError)>),
    /// Something went wrong in the renderer
    RenderError(String),
    /// Something went wrong in the audio playback
    AudioError(String),
    /// Something went wrong trying to set or get window properties.
    WindowError(String),
    /// Something went wrong trying to create a window
    WindowCreationError(winit::CreationError),
    /// Something went wrong trying to read from a file
    IOError(io::Error),
    /// Something went wrong trying to load/render a font
    FontError(String),
    /// Something went wrong applying video settings.
    VideoError(String),
    /// Something went wrong with Gilrs
    GamepadError(String),
}

impl fmt::Display for GameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            GameError::ConfigError(ref s) => write!(f, "Config error: {}", s),
            GameError::ResourceLoadError(ref s) => write!(f, "Error loading resource: {}", s),
            GameError::ResourceNotFound(ref s, ref paths) => write!(
                f,
                "Resource not found: {}, searched in paths {:?}",
                s, paths
            ),
            GameError::WindowError(ref e) => write!(f, "Window creation error: {}", e),
            _ => write!(f, "GameError {:?}", self),
        }
    }
}

impl Error for GameError {
    fn description(&self) -> &str {
        match *self {
            GameError::FilesystemError(_) => "Filesystem error",
            GameError::ConfigError(_) => "Config file error",
            GameError::EventLoopError(_) => "Event loop error",
            GameError::ResourceLoadError(_) => "Resource load error",
            GameError::ResourceNotFound(_, _) => "Resource not found",
            GameError::RenderError(_) => "Render error",
            GameError::AudioError(_) => "Audio error",
            GameError::WindowError(_) => "Window error",
            GameError::WindowCreationError(_) => "Window creation error",
            GameError::IOError(_) => "IO error",
            GameError::FontError(_) => "Font error",
            GameError::VideoError(_) => "Video error",
            GameError::GamepadError(_) => "Gamepad error",
        }
    }

    fn cause(&self) -> Option<&dyn Error> {
        match *self {
            GameError::WindowCreationError(ref e) => Some(e),
            GameError::IOError(ref e) => Some(e),
            _ => None,
        }
    }
}

/// A convenient result type consisting of a return type and a `GameError`
pub type GameResult<T = ()> = Result<T, GameError>;

impl From<AppDirsError> for GameError {
    fn from(e: AppDirsError) -> GameError {
        let errmessage = format!("{}", e);
        GameError::FilesystemError(errmessage)
    }
}
impl From<io::Error> for GameError {
    fn from(e: io::Error) -> GameError {
        GameError::IOError(e)
    }
}

impl From<toml::de::Error> for GameError {
    fn from(e: toml::de::Error) -> GameError {
        let errstr = format!("TOML decode error: {}", e.description());

        GameError::ConfigError(errstr)
    }
}

impl From<toml::ser::Error> for GameError {
    fn from(e: toml::ser::Error) -> GameError {
        let errstr = format!("TOML error (possibly encoding?): {}", e.description());
        GameError::ConfigError(errstr)
    }
}

impl From<zip::result::ZipError> for GameError {
    fn from(e: zip::result::ZipError) -> GameError {
        let errstr = format!("Zip error: {}", e.description());
        GameError::ResourceLoadError(errstr)
    }
}

impl From<DecoderError> for GameError {
    fn from(e: DecoderError) -> GameError {
        let errstr = format!("Audio decoder error: {:?}", e);
        GameError::AudioError(errstr)
    }
}

impl From<image::ImageError> for GameError {
    fn from(e: image::ImageError) -> GameError {
        let errstr = format!("Image load error: {}", e.description());
        GameError::ResourceLoadError(errstr)
    }
}

// TODO: improve winit/glutin error handling.

impl From<winit::EventsLoopClosed> for GameError {
    fn from(_: winit::EventsLoopClosed) -> GameError {
        let e = "An event loop proxy attempted to wake up an event loop that no longer exists."
            .to_owned();
        GameError::EventLoopError(e)
    }
}

impl From<winit::CreationError> for GameError {
    fn from(s: winit::CreationError) -> GameError {
        GameError::WindowCreationError(s)
    }
}

impl From<vk::Result> for GameError {
    fn from(s: vk::Result) -> GameError {
        let errstr = format!("Ash error: {}", s);
        GameError::RenderError(errstr)
    }
}

impl From<ash::InstanceError> for GameError {
    fn from(s: ash::InstanceError) -> GameError {
        let errstr = format!("Ash error: {}", s);
        GameError::RenderError(errstr)
    }
}

impl From<ash::DeviceError> for GameError {
    fn from(s: ash::DeviceError) -> GameError {
        let errstr = format!("Ash error: {}", s);
        GameError::RenderError(errstr)
    }
}

impl From<gilrs::Error> for GameError {
    // TODO: Better error type?
    fn from(s: gilrs::Error) -> GameError {
        let errstr = format!("Gamepad error: {}", s);
        GameError::GamepadError(errstr)
    }
}
