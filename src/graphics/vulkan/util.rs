use ash::version::DeviceV1_0;
use ash::vk;
use std::ptr;
use GameResult;

pub fn find_memory_type_index(
    memory_reqs: &vk::MemoryRequirements,
    pdevice_memory_props: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> GameResult<u32> {
    let mut bits = memory_reqs.memory_type_bits;
    for (index, ty) in pdevice_memory_props.memory_types.iter().enumerate() {
        if (bits & 1) == 1 && ty.property_flags.subset(flags) {
            return Ok(index as u32);
        }
        bits >>= 1;
    }
    unimplemented!("vulkan::util::find_memory_type_index");
}

pub fn single_time_commands<D, F>(
    device: &D,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    wait_mask: &[vk::PipelineStageFlags],
    wait_semaphores: &[vk::Semaphore],
    signal_semaphores: &[vk::Semaphore],
    f: F,
) -> GameResult
where
    D: DeviceV1_0,
    F: FnOnce(&D, vk::CommandBuffer),
{
    let command_buffer = {
        let allocate_info = vk::CommandBufferAllocateInfo {
            s_type: vk::StructureType::CommandBufferAllocateInfo,
            p_next: ptr::null(),
            command_pool,
            level: vk::CommandBufferLevel::Primary,
            command_buffer_count: 1,
        };
        unsafe { device.allocate_command_buffers(&allocate_info)?[0] }
    };
    unsafe {
        device.reset_command_buffer(
            command_buffer,
            vk::COMMAND_BUFFER_RESET_RELEASE_RESOURCES_BIT,
        )?;
    }
    let begin_info = vk::CommandBufferBeginInfo {
        s_type: vk::StructureType::CommandBufferBeginInfo,
        p_next: ptr::null(),
        flags: vk::COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
        p_inheritance_info: ptr::null(),
    };
    unsafe {
        device.begin_command_buffer(command_buffer, &begin_info)?;
    }
    f(device, command_buffer);
    unsafe {
        device.end_command_buffer(command_buffer)?;
    }
    let fence = {
        let create_info = vk::FenceCreateInfo {
            s_type: vk::StructureType::FenceCreateInfo,
            p_next: ptr::null(),
            flags: vk::FenceCreateFlags::empty(),
        };
        unsafe { device.create_fence(&create_info, None)? }
    };
    let submit_info = vk::SubmitInfo {
        s_type: vk::StructureType::SubmitInfo,
        p_next: ptr::null(),
        wait_semaphore_count: wait_semaphores.len() as u32,
        p_wait_semaphores: wait_semaphores.as_ptr(),
        p_wait_dst_stage_mask: wait_mask.as_ptr(),
        command_buffer_count: 1,
        p_command_buffers: &command_buffer,
        signal_semaphore_count: signal_semaphores.len() as u32,
        p_signal_semaphores: signal_semaphores.as_ptr(),
    };
    unsafe {
        device.queue_submit(queue, &[submit_info], fence)?;
        device.wait_for_fences(&[fence], true, !0)?;
        device.destroy_fence(fence, None);
    }
    unsafe {
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    Ok(())
}

pub fn copy_buffer_to_image<D>(
    device: &D,
    command_buffer: vk::CommandBuffer,
    buffer: vk::Buffer,
    image: vk::Image,
    width: u32,
    height: u32,
) where
    D: DeviceV1_0,
{
    let region = vk::BufferImageCopy {
        buffer_offset: 0,
        buffer_row_length: 0,
        buffer_image_height: 0,
        image_subresource: vk::ImageSubresourceLayers {
            aspect_mask: vk::IMAGE_ASPECT_COLOR_BIT,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        },
        image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
        image_extent: vk::Extent3D {
            width,
            height,
            depth: 1,
        },
    };
    unsafe {
        device.cmd_copy_buffer_to_image(
            command_buffer,
            buffer,
            image,
            vk::ImageLayout::TransferDstOptimal,
            &[region],
        );
    }
}

pub fn create_shader_module<D>(device: &D, bytes: &[u8]) -> GameResult<vk::ShaderModule>
where
    D: DeviceV1_0,
{
    // TODO: Test this, it's easier to open a file and go through those bytes
    // let bytes = bytes.iter().filter_map(|b| b.ok()).collect::<Vec<_>>();
    let create_info = vk::ShaderModuleCreateInfo {
        s_type: vk::StructureType::ShaderModuleCreateInfo,
        p_next: ptr::null(),
        flags: vk::ShaderModuleCreateFlags::empty(),
        code_size: bytes.len(),
        p_code: bytes.as_ptr() as _,
    };
    let shader_module = unsafe { device.create_shader_module(&create_info, None)? };
    Ok(shader_module)
}
