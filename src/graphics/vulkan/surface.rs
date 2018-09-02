use ash::extensions::Surface;
use ash::version::{EntryV1_0, InstanceV1_0};
use ash::vk;
use std::ptr;
use winit;
use GameResult;

#[cfg(target_os = "windows")]
pub fn create_surface<E, I>(
    entry: &E,
    instance: &I,
    window: &winit::Window,
) -> GameResult<vk::SurfaceKHR>
where
    E: EntryV1_0,
    I: InstanceV1_0,
{
    use ash::extensions::Win32Surface;
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::GetWindow;
    use winit::os::windows::WindowExt;

    let hwnd = window.get_hwnd() as HWND;
    let hinstance = unsafe { GetWindow(hwnd, 0) as *const vk::c_void };
    let win32_create_info = vk::Win32SurfaceCreateInfoKHR {
        s_type: vk::StructureType::Win32SurfaceCreateInfoKhr,
        p_next: ptr::null(),
        flags: vk::Win32SurfaceCreateFlagsKHR::empty(),
        hinstance: hinstance,
        hwnd: hwnd as *const vk::c_void,
    };
    let loader = Win32Surface::new(entry, instance).expect("Unable to load Win32 surface");
    let surface = unsafe { loader.create_win32_surface_khr(&win32_create_info, None)? };
    Ok(surface)
}

#[cfg(target_os = "windows")]
pub fn instance_extension_names() -> Vec<*const i8> {
    use ash::extensions::Win32Surface;
    vec![Surface::name().as_ptr(), Win32Surface::name().as_ptr()]
}

#[cfg(target_os = "macos")]
pub fn create_surface<E, I>(
    entry: &E,
    instance: &I,
    window: &winit::Window,
) -> GameResult<vk::SurfaceKHR>
where
    E: EntryV1_0,
    I: InstanceV1_0,
{
    use ash::extensions::MacOSSurface;
    use cocoa::appkit::{NSView, NSWindow};
    use cocoa::base::id as cocoa_id;
    use metal::CoreAnimationLayer;
    use objc::runtime::YES;
    use winit::os::macos::WindowExt;

    let wnd: cocoa_id = mem::transmute(window.get_nswindow());
    let layer = CoreAnimationLayer::new();
    layer.set_edge_antialiasing_mask(0);
    layer.set_presents_with_transaction(false);
    layer.remove_all_animations();

    let view = wnd.contentView();
    layer.set_contents_scale(view.backingScaleFactor());
    view.setLayer(mem::transmute(layer.as_ref()));
    view.setWantsLayer(YES);

    let create_info = vk::MacOSSurfaceCreateInfoMVK {
        s_type: vk::StructureType::MacOSSurfaceCreateInfoMvk,
        p_next: ptr::null(),
        flags: Default::default(),
        p_view: window.get_nsview() as *const vk::types::c_void,
    };
    let loader = MacOSSurface::new(entry, instance).expect("Unable to load macOS surface");
    let surface = unsafe { loader.create_macos_surface_mvk(&create_info, None)? };
    Ok(surface)
}

#[cfg(target_os = "macos")]
pub fn instance_extension_names() -> Vec<*const i8> {
    use ash::extensions::MacOSSurface;
    vec![Surface::name().as_ptr(), MacOSSurface::name().as_ptr()]
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
pub fn create_surface<E, I>(
    entry: &E,
    instance: &I,
    window: &winit::Window,
) -> GameResult<vk::SurfaceKHR>
where
    E: EntryV1_0,
    I: InstanceV1_0,
{
    use ash::extensions::XlibSurface;
    use winit::os::unix::WindowExt;

    let x11_display = window.get_xlib_display().expect("Unable to get display");
    let x11_window = window.get_xlib_window().expect("Unable to get window");
    let x11_create_info = vk::XlibSurfaceCreateInfoKHR {
        s_type: vk::StructureType::XlibSurfaceCreateInfoKhr,
        p_next: ptr::null(),
        flags: Default::default(),
        window: x11_window as vk::Window,
        dpy: x11_display as *mut vk::Display,
    };
    let loader = XlibSurface::new(entry, instance).expect("Unable to load xlib surface");
    let surface = unsafe { loader.create_xlib_surface_khr(&x11_create_info, None)? };
    Ok(surface)
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
pub fn instance_extension_names() -> Vec<*const i8> {
    use ash::extensions::XlibSurface;
    vec![Surface::name().as_ptr(), XlibSurface::name().as_ptr()]
}
