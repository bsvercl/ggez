use ash::version::{DeviceV1_0, V1_0};
use ash::vk;
use ash::Device;

pub struct Image {
    device: Device<V1_0>,
    pdevice_memory_props: vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory, None);
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
        }
    }
}
