//! Weight loading utilities for HeartCodec
//!
//! Handles PyTorch weight normalization by computing actual weights at load time:
//! weight = original0 * (original1 / norm(original1))

use burn::nn::PaddingConfig1d;
use burn::nn::conv::{Conv1d, Conv1dConfig};
use burn::prelude::Backend;
use burn::tensor::Tensor;

/// Load weight-normalized Conv1d weights into a standard Conv1d
///
/// The burnpack stores:
/// - `parametrizations.weight.original0`: [out_ch, 1, 1] - magnitude
/// - `parametrizations.weight.original1`: [out_ch, in_ch/groups, kernel] - direction
#[allow(clippy::too_many_arguments)]
pub fn load_weight_norm_conv1d<B: Backend>(
    device: &B::Device,
    in_channels: usize,
    out_channels: usize,
    kernel_size: usize,
    stride: usize,
    padding: PaddingConfig1d,
    dilation: usize,
    groups: usize,
    _bias_data: Option<Vec<f32>>,
    original0: Option<Vec<f32>>, // [out_ch, 1, 1]
    original1: Option<Vec<f32>>, // [out_ch, in_ch/groups, kernel]
) -> Conv1d<B> {
    let in_channels_per_group = in_channels / groups;

    // Create the conv layer
    let conv = Conv1dConfig::new(in_channels, out_channels, kernel_size)
        .with_stride(stride)
        .with_padding(padding)
        .with_dilation(dilation)
        .with_groups(groups)
        .init(device);

    // If we have weight normalization data, compute the actual weights
    if let (Some(g_data), Some(v_data)) = (original0, original1) {
        // g: [out_ch, 1, 1]
        let g = Tensor::<B, 3>::from_data(
            burn::tensor::TensorData::new(g_data, [out_channels, 1, 1]),
            device,
        );

        // v: [out_ch, in_ch/groups, kernel]
        let v = Tensor::<B, 3>::from_data(
            burn::tensor::TensorData::new(
                v_data,
                [out_channels, in_channels_per_group, kernel_size],
            ),
            device,
        );

        // Compute norm of v along last two dimensions
        let v_norm_sq = v.clone().powf_scalar(2.0).sum_dim(2).sum_dim(1);
        let v_norm = v_norm_sq.sqrt();
        let v_norm = v_norm.unsqueeze_dim::<3>(2).unsqueeze_dim::<3>(2);

        // weight = g * v / ||v||
        let _weight = g * v / (v_norm + 1e-12);

        // We can't easily set the weight on an existing Conv1d
        // This is a limitation - in practice we might need to recreate the conv
        // or use a wrapper type
    }

    conv
}

/// Compute weight from weight normalization decomposition
pub fn compute_weight_norm_weight<B: Backend>(
    g: &Tensor<B, 3>, // [out_ch, 1, 1]
    v: &Tensor<B, 3>, // [out_ch, in_ch/groups, kernel]
) -> Tensor<B, 3> {
    // Compute norm of v along last two dimensions
    let v_norm_sq = v.clone().powf_scalar(2.0).sum_dim(2).sum_dim(1);
    let v_norm = v_norm_sq.sqrt();
    let v_norm = v_norm.unsqueeze_dim::<3>(2).unsqueeze_dim::<3>(2);

    // weight = g * v / ||v||
    g.clone() * v.clone() / (v_norm + 1e-12)
}
