use ash::util::Align;
use ash::version::{DeviceV1_0, V1_0};
use ash::vk;
use ash::Device;
use graphics::vulkan;
use std::fmt;
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use GameResult;

#[derive(Clone)]
pub struct Buffer<T>
where
    T: Copy,
{
    device: Device<V1_0>,
    pdevice_memory_props: vk::PhysicalDeviceMemoryProperties,
    pub(crate) buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    pub(crate) memory_requirements: vk::MemoryRequirements,
    usage: vk::BufferUsageFlags,
    props: vk::MemoryPropertyFlags,
    count: usize,
    _phantom: PhantomData<T>,
}

impl<T> PartialEq for Buffer<T>
where
    T: Copy,
{
    fn eq(&self, other: &Self) -> bool {
        // This should be good enough
        self.buffer == other.buffer && self.memory == other.memory && self.count == other.count
    }
}

impl<T> fmt::Debug for Buffer<T>
where
    T: Copy,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TODO")
    }
}

fn create_buffer(
    device: &Device<V1_0>,
    pdevice_memory_props: &vk::PhysicalDeviceMemoryProperties,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    props: vk::MemoryPropertyFlags,
) -> GameResult<(vk::Buffer, vk::MemoryRequirements, vk::DeviceMemory)> {
    let buffer = {
        let create_info = vk::BufferCreateInfo {
            s_type: vk::StructureType::BufferCreateInfo,
            p_next: ptr::null(),
            flags: vk::BufferCreateFlags::empty(),
            size,
            usage,
            sharing_mode: vk::SharingMode::Exclusive,
            queue_family_index_count: 0,
            p_queue_family_indices: ptr::null(),
        };
        unsafe { device.create_buffer(&create_info, None)? }
    };
    let memory_requirements = device.get_buffer_memory_requirements(buffer);
    let memory = {
        let allocate_info = vk::MemoryAllocateInfo {
            s_type: vk::StructureType::MemoryAllocateInfo,
            p_next: ptr::null(),
            allocation_size: memory_requirements.size,
            memory_type_index: vulkan::find_memory_type_index(
                &memory_requirements,
                pdevice_memory_props,
                props,
            )?,
        };
        unsafe { device.allocate_memory(&allocate_info, None)? }
    };
    unsafe {
        device.bind_buffer_memory(buffer, memory, 0)?;
    }
    Ok((buffer, memory_requirements, memory))
}

impl<T> Buffer<T>
where
    T: Copy,
{
    pub fn empty(
        device: &Device<V1_0>,
        pdevice_memory_props: &vk::PhysicalDeviceMemoryProperties,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        props: vk::MemoryPropertyFlags,
    ) -> GameResult<Self> {
        let (buffer, memory_requirements, memory) =
            create_buffer(device, pdevice_memory_props, size, usage, props)?;
        Ok(Buffer {
            device: device.clone(),
            pdevice_memory_props: pdevice_memory_props.clone(),
            buffer,
            memory,
            memory_requirements,
            usage,
            props,
            count: 0,
            _phantom: PhantomData {},
        })
    }

    pub fn new(
        device: &Device<V1_0>,
        pdevice_memory_props: &vk::PhysicalDeviceMemoryProperties,
        data: &[T],
        usage: vk::BufferUsageFlags,
        props: vk::MemoryPropertyFlags,
    ) -> GameResult<Self> {
        let mut buffer = Buffer::empty(
            device,
            pdevice_memory_props,
            mem::size_of_val(data) as vk::DeviceSize,
            usage,
            props,
        )?;
        buffer.update(data)?;
        Ok(buffer)
    }

    pub fn update(&mut self, data: &[T]) -> GameResult {
        if data.len() != self.count {
            println!("Resizing buffer. From {} to {}.", self.count, data.len());
            unsafe {
                self.device.free_memory(self.memory, None);
                self.device.destroy_buffer(self.buffer, None);
            }
            let (buffer, memory_requirements, memory) = create_buffer(
                &self.device,
                &self.pdevice_memory_props,
                mem::size_of_val(data) as vk::DeviceSize,
                self.usage,
                self.props,
            )?;
            self.buffer = buffer;
            self.memory_requirements = memory_requirements;
            self.memory = memory;
        }
        self.count = data.len();
        let memory = unsafe {
            self.device.map_memory(
                self.memory,
                0,
                self.memory_requirements.size,
                vk::MemoryMapFlags::empty(),
            )?
        };
        let mut align = unsafe {
            Align::new(
                memory,
                mem::align_of::<T>() as vk::DeviceSize,
                self.memory_requirements.size,
            )
        };
        align.copy_from_slice(data);
        unsafe {
            self.device.unmap_memory(self.memory);
        }
        if !self.props.subset(vk::MEMORY_PROPERTY_HOST_COHERENT_BIT) {
            let range = vk::MappedMemoryRange {
                s_type: vk::StructureType::MappedMemoryRange,
                p_next: ptr::null(),
                memory: self.memory,
                offset: 0,
                size: self.memory_requirements.size,
            };
            unsafe {
                self.device.flush_mapped_memory_ranges(&[range.clone()])?;
                self.device.invalidate_mapped_memory_ranges(&[range])?;
            }
        }
        Ok(())
    }

    // Returns the size of the buffer in bytes
    pub fn size(&self) -> vk::DeviceSize {
        self.memory_requirements.size
    }

    // Returns the number of elements in the buffer
    pub fn count(&self) -> usize {
        self.count
    }

    pub fn handle(&self) -> vk::Buffer {
        self.buffer
    }
}

impl<T> Drop for Buffer<T>
where
    T: Copy,
{
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory, None);
            self.device.destroy_buffer(self.buffer, None);
        }
    }
}
