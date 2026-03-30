// Generated from ONNX "stable_audio_t5_sim.onnx" by burn-onnx
use burn::prelude::*;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::LinearLayout;
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;


#[derive(Module, Debug)]
pub struct Model<B: Backend> {
    constant1: burn::module::Param<Tensor<B, 1>>,
    constant2: burn::module::Param<Tensor<B, 1>>,
    constant3: burn::module::Param<Tensor<B, 1>>,
    constant4: burn::module::Param<Tensor<B, 1>>,
    constant5: burn::module::Param<Tensor<B, 1>>,
    constant6: burn::module::Param<Tensor<B, 1>>,
    constant7: burn::module::Param<Tensor<B, 1>>,
    constant8: burn::module::Param<Tensor<B, 1>>,
    constant9: burn::module::Param<Tensor<B, 1>>,
    constant10: burn::module::Param<Tensor<B, 1>>,
    constant11: burn::module::Param<Tensor<B, 1>>,
    constant12: burn::module::Param<Tensor<B, 1>>,
    constant13: burn::module::Param<Tensor<B, 1>>,
    constant14: burn::module::Param<Tensor<B, 1>>,
    constant15: burn::module::Param<Tensor<B, 1>>,
    constant16: burn::module::Param<Tensor<B, 1>>,
    constant17: burn::module::Param<Tensor<B, 1>>,
    constant18: burn::module::Param<Tensor<B, 1>>,
    constant19: burn::module::Param<Tensor<B, 1>>,
    constant20: burn::module::Param<Tensor<B, 1>>,
    constant21: burn::module::Param<Tensor<B, 1>>,
    constant22: burn::module::Param<Tensor<B, 1>>,
    constant23: burn::module::Param<Tensor<B, 1>>,
    constant24: burn::module::Param<Tensor<B, 1>>,
    constant25: burn::module::Param<Tensor<B, 1>>,
    constant28: burn::module::Param<Tensor<B, 2>>,
    constant32: burn::module::Param<Tensor<B, 5, Int>>,
    constant75: burn::module::Param<Tensor<B, 2>>,
    constant77: burn::module::Param<Tensor<B, 2>>,
    constant84: burn::module::Param<Tensor<B, 4>>,
    linear1: Linear<B>,
    linear2: Linear<B>,
    linear3: Linear<B>,
    linear4: Linear<B>,
    linear5: Linear<B>,
    linear6: Linear<B>,
    linear7: Linear<B>,
    linear8: Linear<B>,
    linear9: Linear<B>,
    linear10: Linear<B>,
    linear11: Linear<B>,
    linear12: Linear<B>,
    linear13: Linear<B>,
    linear14: Linear<B>,
    linear15: Linear<B>,
    linear16: Linear<B>,
    linear17: Linear<B>,
    linear18: Linear<B>,
    linear19: Linear<B>,
    linear20: Linear<B>,
    linear21: Linear<B>,
    linear22: Linear<B>,
    linear23: Linear<B>,
    linear24: Linear<B>,
    linear25: Linear<B>,
    linear26: Linear<B>,
    linear27: Linear<B>,
    linear28: Linear<B>,
    linear29: Linear<B>,
    linear30: Linear<B>,
    linear31: Linear<B>,
    linear32: Linear<B>,
    linear33: Linear<B>,
    linear34: Linear<B>,
    linear35: Linear<B>,
    linear36: Linear<B>,
    linear37: Linear<B>,
    linear38: Linear<B>,
    linear39: Linear<B>,
    linear40: Linear<B>,
    linear41: Linear<B>,
    linear42: Linear<B>,
    linear43: Linear<B>,
    linear44: Linear<B>,
    linear45: Linear<B>,
    linear46: Linear<B>,
    linear47: Linear<B>,
    linear48: Linear<B>,
    linear49: Linear<B>,
    linear50: Linear<B>,
    phantom: core::marker::PhantomData<B>,
    device: burn::module::Ignored<B::Device>,
}


impl<B: Backend> Default for Model<B> {
    fn default() -> Self {
        Self::from_file("burn_t5/stable_audio_t5_sim.bpk", &Default::default())
    }
}

impl<B: Backend> Model<B> {
    /// Load model weights from a burnpack file.
    pub fn from_file(file: &str, device: &B::Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_file(file);
        model.load_from(&mut store).expect("Failed to load burnpack file");
        model
    }
}

impl<B: Backend> Model<B> {
    #[allow(unused_variables)]
    pub fn new(device: &B::Device) -> Self {
        let constant1: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant2: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant3: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant4: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant5: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant6: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant7: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant8: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant9: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant10: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant11: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant12: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant13: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant14: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant15: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant16: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant17: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant18: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant19: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant20: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant21: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant22: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant23: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant24: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant25: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([768], device),
            device.clone(),
            false,
            [768].into(),
        );
        let constant28: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 2>::zeros([32128, 768], device),
            device.clone(),
            false,
            [32128, 768].into(),
        );
        let constant32: burn::module::Param<Tensor<B, 5, Int>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                5,
                Int,
            >::zeros([1, 1, 1, 128, 2], device),
            device.clone(),
            false,
            [1, 1, 1, 128, 2].into(),
        );
        let constant75: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 2>::zeros([1, 128], device),
            device.clone(),
            false,
            [1, 128].into(),
        );
        let constant77: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 2>::zeros([1, 128], device),
            device.clone(),
            false,
            [1, 128].into(),
        );
        let constant84: burn::module::Param<Tensor<B, 4>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                4,
            >::zeros([1, 12, 128, 128], device),
            device.clone(),
            false,
            [1, 12, 128, 128].into(),
        );
        let linear1 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear2 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear3 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear4 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear5 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear6 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear7 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear8 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear9 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear10 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear11 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear12 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear13 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear14 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear15 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear16 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear17 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear18 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear19 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear20 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear21 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear22 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear23 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear24 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear25 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear26 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear27 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear28 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear29 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear30 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear31 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear32 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear33 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear34 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear35 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear36 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear37 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear38 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear39 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear40 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear41 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear42 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear43 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear44 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear45 = LinearConfig::new(768, 2304).with_bias(false).init(device);
        let linear46 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear47 = LinearConfig::new(768, 3072).with_bias(false).init(device);
        let linear48 = LinearConfig::new(3072, 768).with_bias(false).init(device);
        let linear49 = LinearConfig::new(257, 768)
            .with_bias(true)
            .with_layout(LinearLayout::Col)
            .init(device);
        let linear50 = LinearConfig::new(257, 768)
            .with_bias(true)
            .with_layout(LinearLayout::Col)
            .init(device);
        Self {
            constant1,
            constant2,
            constant3,
            constant4,
            constant5,
            constant6,
            constant7,
            constant8,
            constant9,
            constant10,
            constant11,
            constant12,
            constant13,
            constant14,
            constant15,
            constant16,
            constant17,
            constant18,
            constant19,
            constant20,
            constant21,
            constant22,
            constant23,
            constant24,
            constant25,
            constant28,
            constant32,
            constant75,
            constant77,
            constant84,
            linear1,
            linear2,
            linear3,
            linear4,
            linear5,
            linear6,
            linear7,
            linear8,
            linear9,
            linear10,
            linear11,
            linear12,
            linear13,
            linear14,
            linear15,
            linear16,
            linear17,
            linear18,
            linear19,
            linear20,
            linear21,
            linear22,
            linear23,
            linear24,
            linear25,
            linear26,
            linear27,
            linear28,
            linear29,
            linear30,
            linear31,
            linear32,
            linear33,
            linear34,
            linear35,
            linear36,
            linear37,
            linear38,
            linear39,
            linear40,
            linear41,
            linear42,
            linear43,
            linear44,
            linear45,
            linear46,
            linear47,
            linear48,
            linear49,
            linear50,
            phantom: core::marker::PhantomData,
            device: burn::module::Ignored(device.clone()),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(
        &self,
        input_ids: Tensor<B, 2, Int>,
        attention_mask: Tensor<B, 2, Int>,
        seconds_start: Tensor<B, 1, Int>,
        seconds_total: Tensor<B, 1, Int>,
    ) -> Tensor<B, 3> {
        let constant1_out1 = self.constant1.val();
        let constant2_out1 = self.constant2.val();
        let constant3_out1 = self.constant3.val();
        let constant4_out1 = self.constant4.val();
        let constant5_out1 = self.constant5.val();
        let constant6_out1 = self.constant6.val();
        let constant7_out1 = self.constant7.val();
        let constant8_out1 = self.constant8.val();
        let constant9_out1 = self.constant9.val();
        let constant10_out1 = self.constant10.val();
        let constant11_out1 = self.constant11.val();
        let constant12_out1 = self.constant12.val();
        let constant13_out1 = self.constant13.val();
        let constant14_out1 = self.constant14.val();
        let constant15_out1 = self.constant15.val();
        let constant16_out1 = self.constant16.val();
        let constant17_out1 = self.constant17.val();
        let constant18_out1 = self.constant18.val();
        let constant19_out1 = self.constant19.val();
        let constant20_out1 = self.constant20.val();
        let constant21_out1 = self.constant21.val();
        let constant22_out1 = self.constant22.val();
        let constant23_out1 = self.constant23.val();
        let constant24_out1 = self.constant24.val();
        let constant25_out1 = self.constant25.val();
        let constant28_out1 = self.constant28.val();
        let constant32_out1 = self.constant32.val();
        let constant73_out1 = 512f32;
        let constant75_out1 = self.constant75.val();
        let constant76_out1 = 2f32;
        let constant77_out1 = self.constant77.val();
        let constant79_out1 = -340282350000000000000000000000000000000f32;
        let constant80_out1 = 0.000001f32;
        let constant81_out1 = 3.1415927f32;
        let constant84_out1 = self.constant84.val();
        let cast1_out1 = attention_mask.bool();
        let gather1_out1 = constant28_out1.take::<2, 3>(0, input_ids);
        let gathernd1_out1 = {
            let data_dims = cast1_out1.clone().dims();
            let indices_dims = constant32_out1.dims();
            let indices_data = constant32_out1.to_data().convert::<i64>();
            let indices_values: alloc::vec::Vec<i64> = indices_data
                .into_vec::<i64>()
                .unwrap();
            let r = data_dims.len();
            let q = indices_dims.len();
            let b = 0;
            let k = indices_dims[q - 1];
            let mut data_strides = alloc::vec![1usize; r];
            for i in (0..r.saturating_sub(1)).rev() {
                data_strides[i] = data_strides[i + 1] * data_dims[i + 1];
            }
            let batch_count: usize = if b > 0 {
                data_dims[..b].iter().product()
            } else {
                1
            };
            let lookups_per_batch: usize = indices_dims[b..q - 1].iter().product();
            let slice_size: usize = if b + k < r {
                data_dims[b + k..].iter().product()
            } else {
                1
            };
            let total_data_size: usize = data_dims.iter().product();
            let batch_data_stride: usize = if b > 0 {
                data_dims[b..].iter().product()
            } else {
                total_data_size
            };
            let total_slices = batch_count * lookups_per_batch;
            let output_size = total_slices * slice_size;
            let mut flat_indices: alloc::vec::Vec<i32> = alloc::vec::Vec::with_capacity(
                output_size,
            );
            for bi in 0..batch_count {
                for li in 0..lookups_per_batch {
                    let lookup_idx = bi * lookups_per_batch + li;
                    let mut offset = bi * batch_data_stride;
                    for j in 0..k {
                        let mut idx = indices_values[lookup_idx * k + j];
                        if idx < 0 {
                            idx += data_dims[b + j] as i64;
                        }
                        offset += idx as usize * data_strides[b + j];
                    }
                    for s in 0..slice_size {
                        flat_indices.push((offset + s) as i32);
                    }
                }
            }
            let data_flat = cast1_out1.clone().reshape([total_data_size]);
            let indices_tensor = Tensor::<
                B,
                1,
                Int,
            >::from_data(
                burn::tensor::TensorData::from(flat_indices.as_slice()),
                &*self.device,
            );
            let output_flat = data_flat.select(0, indices_tensor);
            let mut output_shape = [0usize; 4];
            let mut si = 0;
            for i in 0..b {
                output_shape[si] = data_dims[i];
                si += 1;
            }
            for i in b..q - 1 {
                output_shape[si] = indices_dims[i];
                si += 1;
            }
            for i in b + k..r {
                output_shape[si] = data_dims[i];
                si += 1;
            }
            output_flat.reshape(output_shape)
        };
        let constant31_out1 = Tensor::<
            B,
            1,
            Int,
        >::zeros([1], &*self.device)
            .reshape([1, 1, 1, 1])
            .expand([1, 1, 128, 1])
            .equal_elem(0);
        let and1_out1 = constant31_out1.bool_and(gathernd1_out1);
        let where1_out1 = and1_out1
            .float()
            .mul_scalar(-constant79_out1)
            .add_scalar(constant79_out1);
        let pow1_out1 = gather1_out1.clone().powf_scalar(constant76_out1);
        let reducemean1_out1 = { pow1_out1.mean_dim(2usize) };
        let add1_out1 = reducemean1_out1.add_scalar(constant80_out1);
        let sqrt1_out1 = add1_out1.sqrt();
        let reciprocal1_out1 = sqrt1_out1.recip();
        let mul1_out1 = gather1_out1.clone().mul(reciprocal1_out1);
        let mul2_out1 = constant1_out1.unsqueeze_dims(&[0isize, 1isize]).mul(mul1_out1);
        let linear1_out1 = self.linear1.forward(mul2_out1);
        let split_tensors = linear1_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split1_out1, split1_out2, split1_out3] = split_tensors.try_into().unwrap();
        let reshape1_out1 = split1_out1.reshape([1, -1, 12, 64]);
        let transpose1_out1 = reshape1_out1.permute([0, 2, 1, 3]);
        let reshape2_out1 = split1_out2.reshape([1, -1, 12, 64]);
        let reshape3_out1 = split1_out3.reshape([1, -1, 12, 64]);
        let transpose2_out1 = reshape3_out1.permute([0, 2, 1, 3]);
        let transpose3_out1 = reshape2_out1.permute([0, 2, 3, 1]);
        let matmul2_out1 = transpose1_out1.matmul(transpose3_out1);
        let add2_out1 = constant84_out1.add(where1_out1);
        let add3_out1 = matmul2_out1.add(add2_out1.clone());
        let softmax1_out1 = burn::tensor::activation::softmax(add3_out1, 3);
        let matmul3_out1 = softmax1_out1.matmul(transpose2_out1);
        let transpose4_out1 = matmul3_out1.permute([0, 2, 1, 3]);
        let reshape4_out1 = transpose4_out1.reshape([1, -1, 768]);
        let linear2_out1 = self.linear2.forward(reshape4_out1);
        let add4_out1 = gather1_out1.add(linear2_out1);
        let pow2_out1 = add4_out1.clone().powf_scalar(constant76_out1);
        let reducemean2_out1 = { pow2_out1.mean_dim(2usize) };
        let add5_out1 = reducemean2_out1.add_scalar(constant80_out1);
        let sqrt2_out1 = add5_out1.sqrt();
        let reciprocal2_out1 = sqrt2_out1.recip();
        let mul3_out1 = add4_out1.clone().mul(reciprocal2_out1);
        let mul4_out1 = constant2_out1.unsqueeze_dims(&[0isize, 1isize]).mul(mul3_out1);
        let linear3_out1 = self.linear3.forward(mul4_out1);
        let relu1_out1 = burn::tensor::activation::relu(linear3_out1);
        let linear4_out1 = self.linear4.forward(relu1_out1);
        let add6_out1 = add4_out1.add(linear4_out1);
        let pow3_out1 = add6_out1.clone().powf_scalar(constant76_out1);
        let reducemean3_out1 = { pow3_out1.mean_dim(2usize) };
        let add7_out1 = reducemean3_out1.add_scalar(constant80_out1);
        let sqrt3_out1 = add7_out1.sqrt();
        let reciprocal3_out1 = sqrt3_out1.recip();
        let mul5_out1 = add6_out1.clone().mul(reciprocal3_out1);
        let mul6_out1 = constant3_out1.unsqueeze_dims(&[0isize, 1isize]).mul(mul5_out1);
        let linear5_out1 = self.linear5.forward(mul6_out1);
        let split_tensors = linear5_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split2_out1, split2_out2, split2_out3] = split_tensors.try_into().unwrap();
        let reshape5_out1 = split2_out1.reshape([1, -1, 12, 64]);
        let transpose5_out1 = reshape5_out1.permute([0, 2, 1, 3]);
        let reshape6_out1 = split2_out2.reshape([1, -1, 12, 64]);
        let reshape7_out1 = split2_out3.reshape([1, -1, 12, 64]);
        let transpose6_out1 = reshape7_out1.permute([0, 2, 1, 3]);
        let transpose7_out1 = reshape6_out1.permute([0, 2, 3, 1]);
        let matmul8_out1 = transpose5_out1.matmul(transpose7_out1);
        let add8_out1 = matmul8_out1.add(add2_out1.clone());
        let softmax2_out1 = burn::tensor::activation::softmax(add8_out1, 3);
        let matmul9_out1 = softmax2_out1.matmul(transpose6_out1);
        let transpose8_out1 = matmul9_out1.permute([0, 2, 1, 3]);
        let reshape8_out1 = transpose8_out1.reshape([1, -1, 768]);
        let linear6_out1 = self.linear6.forward(reshape8_out1);
        let add9_out1 = add6_out1.add(linear6_out1);
        let pow4_out1 = add9_out1.clone().powf_scalar(constant76_out1);
        let reducemean4_out1 = { pow4_out1.mean_dim(2usize) };
        let add10_out1 = reducemean4_out1.add_scalar(constant80_out1);
        let sqrt4_out1 = add10_out1.sqrt();
        let reciprocal4_out1 = sqrt4_out1.recip();
        let mul7_out1 = add9_out1.clone().mul(reciprocal4_out1);
        let mul8_out1 = constant4_out1.unsqueeze_dims(&[0isize, 1isize]).mul(mul7_out1);
        let linear7_out1 = self.linear7.forward(mul8_out1);
        let relu2_out1 = burn::tensor::activation::relu(linear7_out1);
        let linear8_out1 = self.linear8.forward(relu2_out1);
        let add11_out1 = add9_out1.add(linear8_out1);
        let pow5_out1 = add11_out1.clone().powf_scalar(constant76_out1);
        let reducemean5_out1 = { pow5_out1.mean_dim(2usize) };
        let add12_out1 = reducemean5_out1.add_scalar(constant80_out1);
        let sqrt5_out1 = add12_out1.sqrt();
        let reciprocal5_out1 = sqrt5_out1.recip();
        let mul9_out1 = add11_out1.clone().mul(reciprocal5_out1);
        let mul10_out1 = constant5_out1.unsqueeze_dims(&[0isize, 1isize]).mul(mul9_out1);
        let linear9_out1 = self.linear9.forward(mul10_out1);
        let split_tensors = linear9_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split3_out1, split3_out2, split3_out3] = split_tensors.try_into().unwrap();
        let reshape9_out1 = split3_out1.reshape([1, -1, 12, 64]);
        let transpose9_out1 = reshape9_out1.permute([0, 2, 1, 3]);
        let reshape10_out1 = split3_out2.reshape([1, -1, 12, 64]);
        let reshape11_out1 = split3_out3.reshape([1, -1, 12, 64]);
        let transpose10_out1 = reshape11_out1.permute([0, 2, 1, 3]);
        let transpose11_out1 = reshape10_out1.permute([0, 2, 3, 1]);
        let matmul14_out1 = transpose9_out1.matmul(transpose11_out1);
        let add13_out1 = matmul14_out1.add(add2_out1.clone());
        let softmax3_out1 = burn::tensor::activation::softmax(add13_out1, 3);
        let matmul15_out1 = softmax3_out1.matmul(transpose10_out1);
        let transpose12_out1 = matmul15_out1.permute([0, 2, 1, 3]);
        let reshape12_out1 = transpose12_out1.reshape([1, -1, 768]);
        let linear10_out1 = self.linear10.forward(reshape12_out1);
        let add14_out1 = add11_out1.add(linear10_out1);
        let pow6_out1 = add14_out1.clone().powf_scalar(constant76_out1);
        let reducemean6_out1 = { pow6_out1.mean_dim(2usize) };
        let add15_out1 = reducemean6_out1.add_scalar(constant80_out1);
        let sqrt6_out1 = add15_out1.sqrt();
        let reciprocal6_out1 = sqrt6_out1.recip();
        let mul11_out1 = add14_out1.clone().mul(reciprocal6_out1);
        let mul12_out1 = constant6_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul11_out1);
        let linear11_out1 = self.linear11.forward(mul12_out1);
        let relu3_out1 = burn::tensor::activation::relu(linear11_out1);
        let linear12_out1 = self.linear12.forward(relu3_out1);
        let add16_out1 = add14_out1.add(linear12_out1);
        let pow7_out1 = add16_out1.clone().powf_scalar(constant76_out1);
        let reducemean7_out1 = { pow7_out1.mean_dim(2usize) };
        let add17_out1 = reducemean7_out1.add_scalar(constant80_out1);
        let sqrt7_out1 = add17_out1.sqrt();
        let reciprocal7_out1 = sqrt7_out1.recip();
        let mul13_out1 = add16_out1.clone().mul(reciprocal7_out1);
        let mul14_out1 = constant7_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul13_out1);
        let linear13_out1 = self.linear13.forward(mul14_out1);
        let split_tensors = linear13_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split4_out1, split4_out2, split4_out3] = split_tensors.try_into().unwrap();
        let reshape13_out1 = split4_out1.reshape([1, -1, 12, 64]);
        let transpose13_out1 = reshape13_out1.permute([0, 2, 1, 3]);
        let reshape14_out1 = split4_out2.reshape([1, -1, 12, 64]);
        let reshape15_out1 = split4_out3.reshape([1, -1, 12, 64]);
        let transpose14_out1 = reshape15_out1.permute([0, 2, 1, 3]);
        let transpose15_out1 = reshape14_out1.permute([0, 2, 3, 1]);
        let matmul20_out1 = transpose13_out1.matmul(transpose15_out1);
        let add18_out1 = matmul20_out1.add(add2_out1.clone());
        let softmax4_out1 = burn::tensor::activation::softmax(add18_out1, 3);
        let matmul21_out1 = softmax4_out1.matmul(transpose14_out1);
        let transpose16_out1 = matmul21_out1.permute([0, 2, 1, 3]);
        let reshape16_out1 = transpose16_out1.reshape([1, -1, 768]);
        let linear14_out1 = self.linear14.forward(reshape16_out1);
        let add19_out1 = add16_out1.add(linear14_out1);
        let pow8_out1 = add19_out1.clone().powf_scalar(constant76_out1);
        let reducemean8_out1 = { pow8_out1.mean_dim(2usize) };
        let add20_out1 = reducemean8_out1.add_scalar(constant80_out1);
        let sqrt8_out1 = add20_out1.sqrt();
        let reciprocal8_out1 = sqrt8_out1.recip();
        let mul15_out1 = add19_out1.clone().mul(reciprocal8_out1);
        let mul16_out1 = constant8_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul15_out1);
        let linear15_out1 = self.linear15.forward(mul16_out1);
        let relu4_out1 = burn::tensor::activation::relu(linear15_out1);
        let linear16_out1 = self.linear16.forward(relu4_out1);
        let add21_out1 = add19_out1.add(linear16_out1);
        let pow9_out1 = add21_out1.clone().powf_scalar(constant76_out1);
        let reducemean9_out1 = { pow9_out1.mean_dim(2usize) };
        let add22_out1 = reducemean9_out1.add_scalar(constant80_out1);
        let sqrt9_out1 = add22_out1.sqrt();
        let reciprocal9_out1 = sqrt9_out1.recip();
        let mul17_out1 = add21_out1.clone().mul(reciprocal9_out1);
        let mul18_out1 = constant9_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul17_out1);
        let linear17_out1 = self.linear17.forward(mul18_out1);
        let split_tensors = linear17_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split5_out1, split5_out2, split5_out3] = split_tensors.try_into().unwrap();
        let reshape17_out1 = split5_out1.reshape([1, -1, 12, 64]);
        let transpose17_out1 = reshape17_out1.permute([0, 2, 1, 3]);
        let reshape18_out1 = split5_out2.reshape([1, -1, 12, 64]);
        let reshape19_out1 = split5_out3.reshape([1, -1, 12, 64]);
        let transpose18_out1 = reshape19_out1.permute([0, 2, 1, 3]);
        let transpose19_out1 = reshape18_out1.permute([0, 2, 3, 1]);
        let matmul26_out1 = transpose17_out1.matmul(transpose19_out1);
        let add23_out1 = matmul26_out1.add(add2_out1.clone());
        let softmax5_out1 = burn::tensor::activation::softmax(add23_out1, 3);
        let matmul27_out1 = softmax5_out1.matmul(transpose18_out1);
        let transpose20_out1 = matmul27_out1.permute([0, 2, 1, 3]);
        let reshape20_out1 = transpose20_out1.reshape([1, -1, 768]);
        let linear18_out1 = self.linear18.forward(reshape20_out1);
        let add24_out1 = add21_out1.add(linear18_out1);
        let pow10_out1 = add24_out1.clone().powf_scalar(constant76_out1);
        let reducemean10_out1 = { pow10_out1.mean_dim(2usize) };
        let add25_out1 = reducemean10_out1.add_scalar(constant80_out1);
        let sqrt10_out1 = add25_out1.sqrt();
        let reciprocal10_out1 = sqrt10_out1.recip();
        let mul19_out1 = add24_out1.clone().mul(reciprocal10_out1);
        let mul20_out1 = constant10_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul19_out1);
        let linear19_out1 = self.linear19.forward(mul20_out1);
        let relu5_out1 = burn::tensor::activation::relu(linear19_out1);
        let linear20_out1 = self.linear20.forward(relu5_out1);
        let add26_out1 = add24_out1.add(linear20_out1);
        let pow11_out1 = add26_out1.clone().powf_scalar(constant76_out1);
        let reducemean11_out1 = { pow11_out1.mean_dim(2usize) };
        let add27_out1 = reducemean11_out1.add_scalar(constant80_out1);
        let sqrt11_out1 = add27_out1.sqrt();
        let reciprocal11_out1 = sqrt11_out1.recip();
        let mul21_out1 = add26_out1.clone().mul(reciprocal11_out1);
        let mul22_out1 = constant11_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul21_out1);
        let linear21_out1 = self.linear21.forward(mul22_out1);
        let split_tensors = linear21_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split6_out1, split6_out2, split6_out3] = split_tensors.try_into().unwrap();
        let reshape21_out1 = split6_out1.reshape([1, -1, 12, 64]);
        let transpose21_out1 = reshape21_out1.permute([0, 2, 1, 3]);
        let reshape22_out1 = split6_out2.reshape([1, -1, 12, 64]);
        let reshape23_out1 = split6_out3.reshape([1, -1, 12, 64]);
        let transpose22_out1 = reshape23_out1.permute([0, 2, 1, 3]);
        let transpose23_out1 = reshape22_out1.permute([0, 2, 3, 1]);
        let matmul32_out1 = transpose21_out1.matmul(transpose23_out1);
        let add28_out1 = matmul32_out1.add(add2_out1.clone());
        let softmax6_out1 = burn::tensor::activation::softmax(add28_out1, 3);
        let matmul33_out1 = softmax6_out1.matmul(transpose22_out1);
        let transpose24_out1 = matmul33_out1.permute([0, 2, 1, 3]);
        let reshape24_out1 = transpose24_out1.reshape([1, -1, 768]);
        let linear22_out1 = self.linear22.forward(reshape24_out1);
        let add29_out1 = add26_out1.add(linear22_out1);
        let pow12_out1 = add29_out1.clone().powf_scalar(constant76_out1);
        let reducemean12_out1 = { pow12_out1.mean_dim(2usize) };
        let add30_out1 = reducemean12_out1.add_scalar(constant80_out1);
        let sqrt12_out1 = add30_out1.sqrt();
        let reciprocal12_out1 = sqrt12_out1.recip();
        let mul23_out1 = add29_out1.clone().mul(reciprocal12_out1);
        let mul24_out1 = constant12_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul23_out1);
        let linear23_out1 = self.linear23.forward(mul24_out1);
        let relu6_out1 = burn::tensor::activation::relu(linear23_out1);
        let linear24_out1 = self.linear24.forward(relu6_out1);
        let add31_out1 = add29_out1.add(linear24_out1);
        let pow13_out1 = add31_out1.clone().powf_scalar(constant76_out1);
        let reducemean13_out1 = { pow13_out1.mean_dim(2usize) };
        let add32_out1 = reducemean13_out1.add_scalar(constant80_out1);
        let sqrt13_out1 = add32_out1.sqrt();
        let reciprocal13_out1 = sqrt13_out1.recip();
        let mul25_out1 = add31_out1.clone().mul(reciprocal13_out1);
        let mul26_out1 = constant13_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul25_out1);
        let linear25_out1 = self.linear25.forward(mul26_out1);
        let split_tensors = linear25_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split7_out1, split7_out2, split7_out3] = split_tensors.try_into().unwrap();
        let reshape25_out1 = split7_out1.reshape([1, -1, 12, 64]);
        let transpose25_out1 = reshape25_out1.permute([0, 2, 1, 3]);
        let reshape26_out1 = split7_out2.reshape([1, -1, 12, 64]);
        let reshape27_out1 = split7_out3.reshape([1, -1, 12, 64]);
        let transpose26_out1 = reshape27_out1.permute([0, 2, 1, 3]);
        let transpose27_out1 = reshape26_out1.permute([0, 2, 3, 1]);
        let matmul38_out1 = transpose25_out1.matmul(transpose27_out1);
        let add33_out1 = matmul38_out1.add(add2_out1.clone());
        let softmax7_out1 = burn::tensor::activation::softmax(add33_out1, 3);
        let matmul39_out1 = softmax7_out1.matmul(transpose26_out1);
        let transpose28_out1 = matmul39_out1.permute([0, 2, 1, 3]);
        let reshape28_out1 = transpose28_out1.reshape([1, -1, 768]);
        let linear26_out1 = self.linear26.forward(reshape28_out1);
        let add34_out1 = add31_out1.add(linear26_out1);
        let pow14_out1 = add34_out1.clone().powf_scalar(constant76_out1);
        let reducemean14_out1 = { pow14_out1.mean_dim(2usize) };
        let add35_out1 = reducemean14_out1.add_scalar(constant80_out1);
        let sqrt14_out1 = add35_out1.sqrt();
        let reciprocal14_out1 = sqrt14_out1.recip();
        let mul27_out1 = add34_out1.clone().mul(reciprocal14_out1);
        let mul28_out1 = constant14_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul27_out1);
        let linear27_out1 = self.linear27.forward(mul28_out1);
        let relu7_out1 = burn::tensor::activation::relu(linear27_out1);
        let linear28_out1 = self.linear28.forward(relu7_out1);
        let add36_out1 = add34_out1.add(linear28_out1);
        let pow15_out1 = add36_out1.clone().powf_scalar(constant76_out1);
        let reducemean15_out1 = { pow15_out1.mean_dim(2usize) };
        let add37_out1 = reducemean15_out1.add_scalar(constant80_out1);
        let sqrt15_out1 = add37_out1.sqrt();
        let reciprocal15_out1 = sqrt15_out1.recip();
        let mul29_out1 = add36_out1.clone().mul(reciprocal15_out1);
        let mul30_out1 = constant15_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul29_out1);
        let linear29_out1 = self.linear29.forward(mul30_out1);
        let split_tensors = linear29_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split8_out1, split8_out2, split8_out3] = split_tensors.try_into().unwrap();
        let reshape29_out1 = split8_out1.reshape([1, -1, 12, 64]);
        let transpose29_out1 = reshape29_out1.permute([0, 2, 1, 3]);
        let reshape30_out1 = split8_out2.reshape([1, -1, 12, 64]);
        let reshape31_out1 = split8_out3.reshape([1, -1, 12, 64]);
        let transpose30_out1 = reshape31_out1.permute([0, 2, 1, 3]);
        let transpose31_out1 = reshape30_out1.permute([0, 2, 3, 1]);
        let matmul44_out1 = transpose29_out1.matmul(transpose31_out1);
        let add38_out1 = matmul44_out1.add(add2_out1.clone());
        let softmax8_out1 = burn::tensor::activation::softmax(add38_out1, 3);
        let matmul45_out1 = softmax8_out1.matmul(transpose30_out1);
        let transpose32_out1 = matmul45_out1.permute([0, 2, 1, 3]);
        let reshape32_out1 = transpose32_out1.reshape([1, -1, 768]);
        let linear30_out1 = self.linear30.forward(reshape32_out1);
        let add39_out1 = add36_out1.add(linear30_out1);
        let pow16_out1 = add39_out1.clone().powf_scalar(constant76_out1);
        let reducemean16_out1 = { pow16_out1.mean_dim(2usize) };
        let add40_out1 = reducemean16_out1.add_scalar(constant80_out1);
        let sqrt16_out1 = add40_out1.sqrt();
        let reciprocal16_out1 = sqrt16_out1.recip();
        let mul31_out1 = add39_out1.clone().mul(reciprocal16_out1);
        let mul32_out1 = constant16_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul31_out1);
        let linear31_out1 = self.linear31.forward(mul32_out1);
        let relu8_out1 = burn::tensor::activation::relu(linear31_out1);
        let linear32_out1 = self.linear32.forward(relu8_out1);
        let add41_out1 = add39_out1.add(linear32_out1);
        let pow17_out1 = add41_out1.clone().powf_scalar(constant76_out1);
        let reducemean17_out1 = { pow17_out1.mean_dim(2usize) };
        let add42_out1 = reducemean17_out1.add_scalar(constant80_out1);
        let sqrt17_out1 = add42_out1.sqrt();
        let reciprocal17_out1 = sqrt17_out1.recip();
        let mul33_out1 = add41_out1.clone().mul(reciprocal17_out1);
        let mul34_out1 = constant17_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul33_out1);
        let linear33_out1 = self.linear33.forward(mul34_out1);
        let split_tensors = linear33_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split9_out1, split9_out2, split9_out3] = split_tensors.try_into().unwrap();
        let reshape33_out1 = split9_out1.reshape([1, -1, 12, 64]);
        let transpose33_out1 = reshape33_out1.permute([0, 2, 1, 3]);
        let reshape34_out1 = split9_out2.reshape([1, -1, 12, 64]);
        let reshape35_out1 = split9_out3.reshape([1, -1, 12, 64]);
        let transpose34_out1 = reshape35_out1.permute([0, 2, 1, 3]);
        let transpose35_out1 = reshape34_out1.permute([0, 2, 3, 1]);
        let matmul50_out1 = transpose33_out1.matmul(transpose35_out1);
        let add43_out1 = matmul50_out1.add(add2_out1.clone());
        let softmax9_out1 = burn::tensor::activation::softmax(add43_out1, 3);
        let matmul51_out1 = softmax9_out1.matmul(transpose34_out1);
        let transpose36_out1 = matmul51_out1.permute([0, 2, 1, 3]);
        let reshape36_out1 = transpose36_out1.reshape([1, -1, 768]);
        let linear34_out1 = self.linear34.forward(reshape36_out1);
        let add44_out1 = add41_out1.add(linear34_out1);
        let pow18_out1 = add44_out1.clone().powf_scalar(constant76_out1);
        let reducemean18_out1 = { pow18_out1.mean_dim(2usize) };
        let add45_out1 = reducemean18_out1.add_scalar(constant80_out1);
        let sqrt18_out1 = add45_out1.sqrt();
        let reciprocal18_out1 = sqrt18_out1.recip();
        let mul35_out1 = add44_out1.clone().mul(reciprocal18_out1);
        let mul36_out1 = constant18_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul35_out1);
        let linear35_out1 = self.linear35.forward(mul36_out1);
        let relu9_out1 = burn::tensor::activation::relu(linear35_out1);
        let linear36_out1 = self.linear36.forward(relu9_out1);
        let add46_out1 = add44_out1.add(linear36_out1);
        let pow19_out1 = add46_out1.clone().powf_scalar(constant76_out1);
        let reducemean19_out1 = { pow19_out1.mean_dim(2usize) };
        let add47_out1 = reducemean19_out1.add_scalar(constant80_out1);
        let sqrt19_out1 = add47_out1.sqrt();
        let reciprocal19_out1 = sqrt19_out1.recip();
        let mul37_out1 = add46_out1.clone().mul(reciprocal19_out1);
        let mul38_out1 = constant19_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul37_out1);
        let linear37_out1 = self.linear37.forward(mul38_out1);
        let split_tensors = linear37_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split10_out1, split10_out2, split10_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape37_out1 = split10_out1.reshape([1, -1, 12, 64]);
        let transpose37_out1 = reshape37_out1.permute([0, 2, 1, 3]);
        let reshape38_out1 = split10_out2.reshape([1, -1, 12, 64]);
        let reshape39_out1 = split10_out3.reshape([1, -1, 12, 64]);
        let transpose38_out1 = reshape39_out1.permute([0, 2, 1, 3]);
        let transpose39_out1 = reshape38_out1.permute([0, 2, 3, 1]);
        let matmul56_out1 = transpose37_out1.matmul(transpose39_out1);
        let add48_out1 = matmul56_out1.add(add2_out1.clone());
        let softmax10_out1 = burn::tensor::activation::softmax(add48_out1, 3);
        let matmul57_out1 = softmax10_out1.matmul(transpose38_out1);
        let transpose40_out1 = matmul57_out1.permute([0, 2, 1, 3]);
        let reshape40_out1 = transpose40_out1.reshape([1, -1, 768]);
        let linear38_out1 = self.linear38.forward(reshape40_out1);
        let add49_out1 = add46_out1.add(linear38_out1);
        let pow20_out1 = add49_out1.clone().powf_scalar(constant76_out1);
        let reducemean20_out1 = { pow20_out1.mean_dim(2usize) };
        let add50_out1 = reducemean20_out1.add_scalar(constant80_out1);
        let sqrt20_out1 = add50_out1.sqrt();
        let reciprocal20_out1 = sqrt20_out1.recip();
        let mul39_out1 = add49_out1.clone().mul(reciprocal20_out1);
        let mul40_out1 = constant20_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul39_out1);
        let linear39_out1 = self.linear39.forward(mul40_out1);
        let relu10_out1 = burn::tensor::activation::relu(linear39_out1);
        let linear40_out1 = self.linear40.forward(relu10_out1);
        let add51_out1 = add49_out1.add(linear40_out1);
        let pow21_out1 = add51_out1.clone().powf_scalar(constant76_out1);
        let reducemean21_out1 = { pow21_out1.mean_dim(2usize) };
        let add52_out1 = reducemean21_out1.add_scalar(constant80_out1);
        let sqrt21_out1 = add52_out1.sqrt();
        let reciprocal21_out1 = sqrt21_out1.recip();
        let mul41_out1 = add51_out1.clone().mul(reciprocal21_out1);
        let mul42_out1 = constant21_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul41_out1);
        let linear41_out1 = self.linear41.forward(mul42_out1);
        let split_tensors = linear41_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split11_out1, split11_out2, split11_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape41_out1 = split11_out1.reshape([1, -1, 12, 64]);
        let transpose41_out1 = reshape41_out1.permute([0, 2, 1, 3]);
        let reshape42_out1 = split11_out2.reshape([1, -1, 12, 64]);
        let reshape43_out1 = split11_out3.reshape([1, -1, 12, 64]);
        let transpose42_out1 = reshape43_out1.permute([0, 2, 1, 3]);
        let transpose43_out1 = reshape42_out1.permute([0, 2, 3, 1]);
        let matmul62_out1 = transpose41_out1.matmul(transpose43_out1);
        let add53_out1 = matmul62_out1.add(add2_out1.clone());
        let softmax11_out1 = burn::tensor::activation::softmax(add53_out1, 3);
        let matmul63_out1 = softmax11_out1.matmul(transpose42_out1);
        let transpose44_out1 = matmul63_out1.permute([0, 2, 1, 3]);
        let reshape44_out1 = transpose44_out1.reshape([1, -1, 768]);
        let linear42_out1 = self.linear42.forward(reshape44_out1);
        let add54_out1 = add51_out1.add(linear42_out1);
        let pow22_out1 = add54_out1.clone().powf_scalar(constant76_out1);
        let reducemean22_out1 = { pow22_out1.mean_dim(2usize) };
        let add55_out1 = reducemean22_out1.add_scalar(constant80_out1);
        let sqrt22_out1 = add55_out1.sqrt();
        let reciprocal22_out1 = sqrt22_out1.recip();
        let mul43_out1 = add54_out1.clone().mul(reciprocal22_out1);
        let mul44_out1 = constant22_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul43_out1);
        let linear43_out1 = self.linear43.forward(mul44_out1);
        let relu11_out1 = burn::tensor::activation::relu(linear43_out1);
        let linear44_out1 = self.linear44.forward(relu11_out1);
        let add56_out1 = add54_out1.add(linear44_out1);
        let pow23_out1 = add56_out1.clone().powf_scalar(constant76_out1);
        let reducemean23_out1 = { pow23_out1.mean_dim(2usize) };
        let add57_out1 = reducemean23_out1.add_scalar(constant80_out1);
        let sqrt23_out1 = add57_out1.sqrt();
        let reciprocal23_out1 = sqrt23_out1.recip();
        let mul45_out1 = add56_out1.clone().mul(reciprocal23_out1);
        let mul46_out1 = constant23_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul45_out1);
        let linear45_out1 = self.linear45.forward(mul46_out1);
        let split_tensors = linear45_out1.split_with_sizes([768, 768, 768].into(), 2);
        let [split12_out1, split12_out2, split12_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape45_out1 = split12_out1.reshape([1, -1, 12, 64]);
        let transpose45_out1 = reshape45_out1.permute([0, 2, 1, 3]);
        let reshape46_out1 = split12_out2.reshape([1, -1, 12, 64]);
        let reshape47_out1 = split12_out3.reshape([1, -1, 12, 64]);
        let transpose46_out1 = reshape47_out1.permute([0, 2, 1, 3]);
        let transpose47_out1 = reshape46_out1.permute([0, 2, 3, 1]);
        let matmul68_out1 = transpose45_out1.matmul(transpose47_out1);
        let add58_out1 = matmul68_out1.add(add2_out1);
        let softmax12_out1 = burn::tensor::activation::softmax(add58_out1, 3);
        let matmul69_out1 = softmax12_out1.matmul(transpose46_out1);
        let transpose48_out1 = matmul69_out1.permute([0, 2, 1, 3]);
        let reshape48_out1 = transpose48_out1.reshape([1, -1, 768]);
        let linear46_out1 = self.linear46.forward(reshape48_out1);
        let add59_out1 = add56_out1.add(linear46_out1);
        let pow24_out1 = add59_out1.clone().powf_scalar(constant76_out1);
        let reducemean24_out1 = { pow24_out1.mean_dim(2usize) };
        let add60_out1 = reducemean24_out1.add_scalar(constant80_out1);
        let sqrt24_out1 = add60_out1.sqrt();
        let reciprocal24_out1 = sqrt24_out1.recip();
        let mul47_out1 = add59_out1.clone().mul(reciprocal24_out1);
        let mul48_out1 = constant24_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul47_out1);
        let linear47_out1 = self.linear47.forward(mul48_out1);
        let relu12_out1 = burn::tensor::activation::relu(linear47_out1);
        let linear48_out1 = self.linear48.forward(relu12_out1);
        let add61_out1 = add59_out1.add(linear48_out1);
        let pow25_out1 = add61_out1.clone().powf_scalar(constant76_out1);
        let reducemean25_out1 = { pow25_out1.mean_dim(2usize) };
        let add62_out1 = reducemean25_out1.add_scalar(constant80_out1);
        let sqrt25_out1 = add62_out1.sqrt();
        let reciprocal25_out1 = sqrt25_out1.recip();
        let mul49_out1 = add61_out1.mul(reciprocal25_out1);
        let mul50_out1 = constant25_out1
            .unsqueeze_dims(&[0isize, 1isize])
            .mul(mul49_out1);
        let unsqueeze1_out1: Tensor<B, 3, Bool> = cast1_out1.unsqueeze_dims::<3>(&[-1]);
        let cast2_out1 = unsqueeze1_out1.float();
        let mul51_out1 = mul50_out1.mul(cast2_out1);
        let cast3_out1 = seconds_start.float();
        let clip1_out1 = cast3_out1.clamp(0f64, 512f64);
        let div1_out1 = clip1_out1.div_scalar(constant73_out1);
        let reshape49_out1 = div1_out1.reshape([1, 1]);
        let mul52_out1 = reshape49_out1.clone().mul(constant75_out1);
        let mul53_out1 = mul52_out1.mul_scalar(constant76_out1);
        let mul54_out1 = mul53_out1.mul_scalar(constant81_out1);
        let sin1_out1 = mul54_out1.clone().sin();
        let cos1_out1 = mul54_out1.cos();
        let concat1_out1 = burn::tensor::Tensor::cat(
            [reshape49_out1, sin1_out1, cos1_out1].into(),
            1,
        );
        let linear49_out1 = self.linear49.forward(concat1_out1);
        let unsqueeze2_out1: Tensor<B, 3> = linear49_out1.unsqueeze_dims::<3>(&[1]);
        let cast4_out1 = seconds_total.float();
        let clip2_out1 = cast4_out1.clamp(0f64, 512f64);
        let div2_out1 = clip2_out1.div_scalar(constant73_out1);
        let reshape50_out1 = div2_out1.reshape([1, 1]);
        let mul55_out1 = reshape50_out1.clone().mul(constant77_out1);
        let mul56_out1 = mul55_out1.mul_scalar(constant76_out1);
        let mul57_out1 = mul56_out1.mul_scalar(constant81_out1);
        let sin2_out1 = mul57_out1.clone().sin();
        let cos2_out1 = mul57_out1.cos();
        let concat2_out1 = burn::tensor::Tensor::cat(
            [reshape50_out1, sin2_out1, cos2_out1].into(),
            1,
        );
        let linear50_out1 = self.linear50.forward(concat2_out1);
        let unsqueeze3_out1: Tensor<B, 3> = linear50_out1.unsqueeze_dims::<3>(&[1]);
        let concat3_out1 = burn::tensor::Tensor::cat(
            [mul51_out1, unsqueeze2_out1, unsqueeze3_out1].into(),
            1,
        );
        concat3_out1
    }
}
