// Generated from ONNX "stable_audio_vae_decoder_sim.onnx" by burn-onnx
use burn::prelude::*;
use burn::nn::PaddingConfig1d;
use burn::nn::conv::Conv1d;
use burn::nn::conv::Conv1dConfig;
use burn::nn::conv::ConvTranspose1d;
use burn::nn::conv::ConvTranspose1dConfig;
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;


#[derive(Module, Debug)]
pub struct Model<B: Backend> {
    constant37: burn::module::Param<Tensor<B, 3>>,
    constant38: burn::module::Param<Tensor<B, 3>>,
    constant39: burn::module::Param<Tensor<B, 3>>,
    constant40: burn::module::Param<Tensor<B, 3>>,
    constant41: burn::module::Param<Tensor<B, 3>>,
    constant42: burn::module::Param<Tensor<B, 3>>,
    constant43: burn::module::Param<Tensor<B, 3>>,
    constant44: burn::module::Param<Tensor<B, 3>>,
    constant45: burn::module::Param<Tensor<B, 3>>,
    constant46: burn::module::Param<Tensor<B, 3>>,
    constant47: burn::module::Param<Tensor<B, 3>>,
    constant48: burn::module::Param<Tensor<B, 3>>,
    constant49: burn::module::Param<Tensor<B, 3>>,
    constant50: burn::module::Param<Tensor<B, 3>>,
    constant51: burn::module::Param<Tensor<B, 3>>,
    constant52: burn::module::Param<Tensor<B, 3>>,
    constant53: burn::module::Param<Tensor<B, 3>>,
    constant54: burn::module::Param<Tensor<B, 3>>,
    constant55: burn::module::Param<Tensor<B, 3>>,
    constant56: burn::module::Param<Tensor<B, 3>>,
    constant57: burn::module::Param<Tensor<B, 3>>,
    constant58: burn::module::Param<Tensor<B, 3>>,
    constant59: burn::module::Param<Tensor<B, 3>>,
    constant60: burn::module::Param<Tensor<B, 3>>,
    constant61: burn::module::Param<Tensor<B, 3>>,
    constant62: burn::module::Param<Tensor<B, 3>>,
    constant63: burn::module::Param<Tensor<B, 3>>,
    constant64: burn::module::Param<Tensor<B, 3>>,
    constant65: burn::module::Param<Tensor<B, 3>>,
    constant66: burn::module::Param<Tensor<B, 3>>,
    constant67: burn::module::Param<Tensor<B, 3>>,
    constant68: burn::module::Param<Tensor<B, 3>>,
    constant69: burn::module::Param<Tensor<B, 3>>,
    constant70: burn::module::Param<Tensor<B, 3>>,
    constant71: burn::module::Param<Tensor<B, 3>>,
    constant72: burn::module::Param<Tensor<B, 3>>,
    constant73: burn::module::Param<Tensor<B, 3>>,
    constant74: burn::module::Param<Tensor<B, 3>>,
    constant75: burn::module::Param<Tensor<B, 3>>,
    constant76: burn::module::Param<Tensor<B, 3>>,
    constant77: burn::module::Param<Tensor<B, 3>>,
    constant78: burn::module::Param<Tensor<B, 3>>,
    conv1d1: Conv1d<B>,
    convtranspose1d1: ConvTranspose1d<B>,
    conv1d2: Conv1d<B>,
    conv1d3: Conv1d<B>,
    conv1d4: Conv1d<B>,
    conv1d5: Conv1d<B>,
    conv1d6: Conv1d<B>,
    conv1d7: Conv1d<B>,
    convtranspose1d2: ConvTranspose1d<B>,
    conv1d8: Conv1d<B>,
    conv1d9: Conv1d<B>,
    conv1d10: Conv1d<B>,
    conv1d11: Conv1d<B>,
    conv1d12: Conv1d<B>,
    conv1d13: Conv1d<B>,
    convtranspose1d3: ConvTranspose1d<B>,
    conv1d14: Conv1d<B>,
    conv1d15: Conv1d<B>,
    conv1d16: Conv1d<B>,
    conv1d17: Conv1d<B>,
    conv1d18: Conv1d<B>,
    conv1d19: Conv1d<B>,
    convtranspose1d4: ConvTranspose1d<B>,
    conv1d20: Conv1d<B>,
    conv1d21: Conv1d<B>,
    conv1d22: Conv1d<B>,
    conv1d23: Conv1d<B>,
    conv1d24: Conv1d<B>,
    conv1d25: Conv1d<B>,
    convtranspose1d5: ConvTranspose1d<B>,
    conv1d26: Conv1d<B>,
    conv1d27: Conv1d<B>,
    conv1d28: Conv1d<B>,
    conv1d29: Conv1d<B>,
    conv1d30: Conv1d<B>,
    conv1d31: Conv1d<B>,
    conv1d32: Conv1d<B>,
    phantom: core::marker::PhantomData<B>,
    device: burn::module::Ignored<B::Device>,
}


impl<B: Backend> Default for Model<B> {
    fn default() -> Self {
        Self::from_file("burn_vae/stable_audio_vae_decoder_sim.bpk", &Default::default())
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
        let constant37: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 2048, 1], device),
            device.clone(),
            false,
            [1, 2048, 1].into(),
        );
        let constant38: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 2048, 1], device),
            device.clone(),
            false,
            [1, 2048, 1].into(),
        );
        let constant39: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant40: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant41: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant42: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant43: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant44: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant45: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant46: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 1024, 1], device),
            device.clone(),
            false,
            [1, 1024, 1].into(),
        );
        let constant47: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant48: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant49: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant50: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant51: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant52: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant53: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant54: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 512, 1], device),
            device.clone(),
            false,
            [1, 512, 1].into(),
        );
        let constant55: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant56: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant57: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant58: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant59: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant60: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant61: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant62: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 256, 1], device),
            device.clone(),
            false,
            [1, 256, 1].into(),
        );
        let constant63: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant64: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant65: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant66: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant67: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant68: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant69: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant70: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant71: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant72: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant73: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant74: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant75: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant76: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant77: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let constant78: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 3>::zeros([1, 128, 1], device),
            device.clone(),
            false,
            [1, 128, 1].into(),
        );
        let conv1d1 = Conv1dConfig::new(64, 2048, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let convtranspose1d1 = ConvTranspose1dConfig::new([2048, 1024], 16)
            .with_stride(8)
            .with_padding(4)
            .with_padding_out(0)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d2 = Conv1dConfig::new(1024, 1024, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d3 = Conv1dConfig::new(1024, 1024, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d4 = Conv1dConfig::new(1024, 1024, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(9, 9))
            .with_dilation(3)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d5 = Conv1dConfig::new(1024, 1024, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d6 = Conv1dConfig::new(1024, 1024, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(27, 27))
            .with_dilation(9)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d7 = Conv1dConfig::new(1024, 1024, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let convtranspose1d2 = ConvTranspose1dConfig::new([1024, 512], 16)
            .with_stride(8)
            .with_padding(4)
            .with_padding_out(0)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d8 = Conv1dConfig::new(512, 512, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d9 = Conv1dConfig::new(512, 512, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d10 = Conv1dConfig::new(512, 512, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(9, 9))
            .with_dilation(3)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d11 = Conv1dConfig::new(512, 512, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d12 = Conv1dConfig::new(512, 512, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(27, 27))
            .with_dilation(9)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d13 = Conv1dConfig::new(512, 512, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let convtranspose1d3 = ConvTranspose1dConfig::new([512, 256], 8)
            .with_stride(4)
            .with_padding(2)
            .with_padding_out(0)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d14 = Conv1dConfig::new(256, 256, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d15 = Conv1dConfig::new(256, 256, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d16 = Conv1dConfig::new(256, 256, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(9, 9))
            .with_dilation(3)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d17 = Conv1dConfig::new(256, 256, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d18 = Conv1dConfig::new(256, 256, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(27, 27))
            .with_dilation(9)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d19 = Conv1dConfig::new(256, 256, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let convtranspose1d4 = ConvTranspose1dConfig::new([256, 128], 8)
            .with_stride(4)
            .with_padding(2)
            .with_padding_out(0)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d20 = Conv1dConfig::new(128, 128, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d21 = Conv1dConfig::new(128, 128, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d22 = Conv1dConfig::new(128, 128, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(9, 9))
            .with_dilation(3)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d23 = Conv1dConfig::new(128, 128, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d24 = Conv1dConfig::new(128, 128, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(27, 27))
            .with_dilation(9)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d25 = Conv1dConfig::new(128, 128, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let convtranspose1d5 = ConvTranspose1dConfig::new([128, 128], 4)
            .with_stride(2)
            .with_padding(1)
            .with_padding_out(0)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d26 = Conv1dConfig::new(128, 128, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d27 = Conv1dConfig::new(128, 128, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d28 = Conv1dConfig::new(128, 128, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(9, 9))
            .with_dilation(3)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d29 = Conv1dConfig::new(128, 128, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d30 = Conv1dConfig::new(128, 128, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(27, 27))
            .with_dilation(9)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d31 = Conv1dConfig::new(128, 128, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv1d32 = Conv1dConfig::new(128, 2, 7)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Explicit(3, 3))
            .with_dilation(1)
            .with_groups(1)
            .with_bias(false)
            .init(device);
        Self {
            constant37,
            constant38,
            constant39,
            constant40,
            constant41,
            constant42,
            constant43,
            constant44,
            constant45,
            constant46,
            constant47,
            constant48,
            constant49,
            constant50,
            constant51,
            constant52,
            constant53,
            constant54,
            constant55,
            constant56,
            constant57,
            constant58,
            constant59,
            constant60,
            constant61,
            constant62,
            constant63,
            constant64,
            constant65,
            constant66,
            constant67,
            constant68,
            constant69,
            constant70,
            constant71,
            constant72,
            constant73,
            constant74,
            constant75,
            constant76,
            constant77,
            constant78,
            conv1d1,
            convtranspose1d1,
            conv1d2,
            conv1d3,
            conv1d4,
            conv1d5,
            conv1d6,
            conv1d7,
            convtranspose1d2,
            conv1d8,
            conv1d9,
            conv1d10,
            conv1d11,
            conv1d12,
            conv1d13,
            convtranspose1d3,
            conv1d14,
            conv1d15,
            conv1d16,
            conv1d17,
            conv1d18,
            conv1d19,
            convtranspose1d4,
            conv1d20,
            conv1d21,
            conv1d22,
            conv1d23,
            conv1d24,
            conv1d25,
            convtranspose1d5,
            conv1d26,
            conv1d27,
            conv1d28,
            conv1d29,
            conv1d30,
            conv1d31,
            conv1d32,
            phantom: core::marker::PhantomData,
            device: burn::module::Ignored(device.clone()),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(&self, latent_input: Tensor<B, 3>) -> Tensor<B, 3> {
        let constant37_out1 = self.constant37.val();
        let constant38_out1 = self.constant38.val();
        let constant39_out1 = self.constant39.val();
        let constant40_out1 = self.constant40.val();
        let constant41_out1 = self.constant41.val();
        let constant42_out1 = self.constant42.val();
        let constant43_out1 = self.constant43.val();
        let constant44_out1 = self.constant44.val();
        let constant45_out1 = self.constant45.val();
        let constant46_out1 = self.constant46.val();
        let constant47_out1 = self.constant47.val();
        let constant48_out1 = self.constant48.val();
        let constant49_out1 = self.constant49.val();
        let constant50_out1 = self.constant50.val();
        let constant51_out1 = self.constant51.val();
        let constant52_out1 = self.constant52.val();
        let constant53_out1 = self.constant53.val();
        let constant54_out1 = self.constant54.val();
        let constant55_out1 = self.constant55.val();
        let constant56_out1 = self.constant56.val();
        let constant57_out1 = self.constant57.val();
        let constant58_out1 = self.constant58.val();
        let constant59_out1 = self.constant59.val();
        let constant60_out1 = self.constant60.val();
        let constant61_out1 = self.constant61.val();
        let constant62_out1 = self.constant62.val();
        let constant63_out1 = self.constant63.val();
        let constant64_out1 = self.constant64.val();
        let constant65_out1 = self.constant65.val();
        let constant66_out1 = self.constant66.val();
        let constant67_out1 = self.constant67.val();
        let constant68_out1 = self.constant68.val();
        let constant69_out1 = self.constant69.val();
        let constant70_out1 = self.constant70.val();
        let constant71_out1 = self.constant71.val();
        let constant72_out1 = self.constant72.val();
        let constant73_out1 = self.constant73.val();
        let constant74_out1 = self.constant74.val();
        let constant75_out1 = self.constant75.val();
        let constant76_out1 = self.constant76.val();
        let constant77_out1 = self.constant77.val();
        let constant78_out1 = self.constant78.val();
        let constant80_out1 = 2f32;
        let conv1d1_out1 = self.conv1d1.forward(latent_input);
        let mul1_out1 = conv1d1_out1.clone().mul(constant37_out1);
        let sin1_out1 = mul1_out1.sin();
        let pow1_out1 = sin1_out1.powf_scalar(constant80_out1);
        let mul2_out1 = constant38_out1.mul(pow1_out1);
        let add1_out1 = conv1d1_out1.add(mul2_out1);
        let convtranspose1d1_out1 = self.convtranspose1d1.forward(add1_out1);
        let mul3_out1 = convtranspose1d1_out1.clone().mul(constant39_out1.clone());
        let sin2_out1 = mul3_out1.sin();
        let pow2_out1 = sin2_out1.powf_scalar(constant80_out1);
        let mul4_out1 = constant40_out1.clone().mul(pow2_out1);
        let add2_out1 = convtranspose1d1_out1.clone().add(mul4_out1);
        let conv1d2_out1 = self.conv1d2.forward(add2_out1);
        let mul5_out1 = conv1d2_out1.clone().mul(constant39_out1);
        let sin3_out1 = mul5_out1.sin();
        let pow3_out1 = sin3_out1.powf_scalar(constant80_out1);
        let mul6_out1 = constant40_out1.mul(pow3_out1);
        let add3_out1 = conv1d2_out1.add(mul6_out1);
        let conv1d3_out1 = self.conv1d3.forward(add3_out1);
        let add4_out1 = conv1d3_out1.add(convtranspose1d1_out1);
        let mul7_out1 = add4_out1.clone().mul(constant41_out1.clone());
        let sin4_out1 = mul7_out1.sin();
        let pow4_out1 = sin4_out1.powf_scalar(constant80_out1);
        let mul8_out1 = constant42_out1.clone().mul(pow4_out1);
        let add5_out1 = add4_out1.clone().add(mul8_out1);
        let conv1d4_out1 = self.conv1d4.forward(add5_out1);
        let mul9_out1 = conv1d4_out1.clone().mul(constant41_out1);
        let sin5_out1 = mul9_out1.sin();
        let pow5_out1 = sin5_out1.powf_scalar(constant80_out1);
        let mul10_out1 = constant42_out1.mul(pow5_out1);
        let add6_out1 = conv1d4_out1.add(mul10_out1);
        let conv1d5_out1 = self.conv1d5.forward(add6_out1);
        let add7_out1 = conv1d5_out1.add(add4_out1);
        let mul11_out1 = add7_out1.clone().mul(constant43_out1.clone());
        let sin6_out1 = mul11_out1.sin();
        let pow6_out1 = sin6_out1.powf_scalar(constant80_out1);
        let mul12_out1 = constant44_out1.clone().mul(pow6_out1);
        let add8_out1 = add7_out1.clone().add(mul12_out1);
        let conv1d6_out1 = self.conv1d6.forward(add8_out1);
        let mul13_out1 = conv1d6_out1.clone().mul(constant43_out1);
        let sin7_out1 = mul13_out1.sin();
        let pow7_out1 = sin7_out1.powf_scalar(constant80_out1);
        let mul14_out1 = constant44_out1.mul(pow7_out1);
        let add9_out1 = conv1d6_out1.add(mul14_out1);
        let conv1d7_out1 = self.conv1d7.forward(add9_out1);
        let add10_out1 = conv1d7_out1.add(add7_out1);
        let mul15_out1 = add10_out1.clone().mul(constant45_out1);
        let sin8_out1 = mul15_out1.sin();
        let pow8_out1 = sin8_out1.powf_scalar(constant80_out1);
        let mul16_out1 = constant46_out1.mul(pow8_out1);
        let add11_out1 = add10_out1.add(mul16_out1);
        let convtranspose1d2_out1 = self.convtranspose1d2.forward(add11_out1);
        let mul17_out1 = convtranspose1d2_out1.clone().mul(constant47_out1.clone());
        let sin9_out1 = mul17_out1.sin();
        let pow9_out1 = sin9_out1.powf_scalar(constant80_out1);
        let mul18_out1 = constant48_out1.clone().mul(pow9_out1);
        let add12_out1 = convtranspose1d2_out1.clone().add(mul18_out1);
        let conv1d8_out1 = self.conv1d8.forward(add12_out1);
        let mul19_out1 = conv1d8_out1.clone().mul(constant47_out1);
        let sin10_out1 = mul19_out1.sin();
        let pow10_out1 = sin10_out1.powf_scalar(constant80_out1);
        let mul20_out1 = constant48_out1.mul(pow10_out1);
        let add13_out1 = conv1d8_out1.add(mul20_out1);
        let conv1d9_out1 = self.conv1d9.forward(add13_out1);
        let add14_out1 = conv1d9_out1.add(convtranspose1d2_out1);
        let mul21_out1 = add14_out1.clone().mul(constant49_out1.clone());
        let sin11_out1 = mul21_out1.sin();
        let pow11_out1 = sin11_out1.powf_scalar(constant80_out1);
        let mul22_out1 = constant50_out1.clone().mul(pow11_out1);
        let add15_out1 = add14_out1.clone().add(mul22_out1);
        let conv1d10_out1 = self.conv1d10.forward(add15_out1);
        let mul23_out1 = conv1d10_out1.clone().mul(constant49_out1);
        let sin12_out1 = mul23_out1.sin();
        let pow12_out1 = sin12_out1.powf_scalar(constant80_out1);
        let mul24_out1 = constant50_out1.mul(pow12_out1);
        let add16_out1 = conv1d10_out1.add(mul24_out1);
        let conv1d11_out1 = self.conv1d11.forward(add16_out1);
        let add17_out1 = conv1d11_out1.add(add14_out1);
        let mul25_out1 = add17_out1.clone().mul(constant51_out1.clone());
        let sin13_out1 = mul25_out1.sin();
        let pow13_out1 = sin13_out1.powf_scalar(constant80_out1);
        let mul26_out1 = constant52_out1.clone().mul(pow13_out1);
        let add18_out1 = add17_out1.clone().add(mul26_out1);
        let conv1d12_out1 = self.conv1d12.forward(add18_out1);
        let mul27_out1 = conv1d12_out1.clone().mul(constant51_out1);
        let sin14_out1 = mul27_out1.sin();
        let pow14_out1 = sin14_out1.powf_scalar(constant80_out1);
        let mul28_out1 = constant52_out1.mul(pow14_out1);
        let add19_out1 = conv1d12_out1.add(mul28_out1);
        let conv1d13_out1 = self.conv1d13.forward(add19_out1);
        let add20_out1 = conv1d13_out1.add(add17_out1);
        let mul29_out1 = add20_out1.clone().mul(constant53_out1);
        let sin15_out1 = mul29_out1.sin();
        let pow15_out1 = sin15_out1.powf_scalar(constant80_out1);
        let mul30_out1 = constant54_out1.mul(pow15_out1);
        let add21_out1 = add20_out1.add(mul30_out1);
        let convtranspose1d3_out1 = self.convtranspose1d3.forward(add21_out1);
        let mul31_out1 = convtranspose1d3_out1.clone().mul(constant55_out1.clone());
        let sin16_out1 = mul31_out1.sin();
        let pow16_out1 = sin16_out1.powf_scalar(constant80_out1);
        let mul32_out1 = constant56_out1.clone().mul(pow16_out1);
        let add22_out1 = convtranspose1d3_out1.clone().add(mul32_out1);
        let conv1d14_out1 = self.conv1d14.forward(add22_out1);
        let mul33_out1 = conv1d14_out1.clone().mul(constant55_out1);
        let sin17_out1 = mul33_out1.sin();
        let pow17_out1 = sin17_out1.powf_scalar(constant80_out1);
        let mul34_out1 = constant56_out1.mul(pow17_out1);
        let add23_out1 = conv1d14_out1.add(mul34_out1);
        let conv1d15_out1 = self.conv1d15.forward(add23_out1);
        let add24_out1 = conv1d15_out1.add(convtranspose1d3_out1);
        let mul35_out1 = add24_out1.clone().mul(constant57_out1.clone());
        let sin18_out1 = mul35_out1.sin();
        let pow18_out1 = sin18_out1.powf_scalar(constant80_out1);
        let mul36_out1 = constant58_out1.clone().mul(pow18_out1);
        let add25_out1 = add24_out1.clone().add(mul36_out1);
        let conv1d16_out1 = self.conv1d16.forward(add25_out1);
        let mul37_out1 = conv1d16_out1.clone().mul(constant57_out1);
        let sin19_out1 = mul37_out1.sin();
        let pow19_out1 = sin19_out1.powf_scalar(constant80_out1);
        let mul38_out1 = constant58_out1.mul(pow19_out1);
        let add26_out1 = conv1d16_out1.add(mul38_out1);
        let conv1d17_out1 = self.conv1d17.forward(add26_out1);
        let add27_out1 = conv1d17_out1.add(add24_out1);
        let mul39_out1 = add27_out1.clone().mul(constant59_out1.clone());
        let sin20_out1 = mul39_out1.sin();
        let pow20_out1 = sin20_out1.powf_scalar(constant80_out1);
        let mul40_out1 = constant60_out1.clone().mul(pow20_out1);
        let add28_out1 = add27_out1.clone().add(mul40_out1);
        let conv1d18_out1 = self.conv1d18.forward(add28_out1);
        let mul41_out1 = conv1d18_out1.clone().mul(constant59_out1);
        let sin21_out1 = mul41_out1.sin();
        let pow21_out1 = sin21_out1.powf_scalar(constant80_out1);
        let mul42_out1 = constant60_out1.mul(pow21_out1);
        let add29_out1 = conv1d18_out1.add(mul42_out1);
        let conv1d19_out1 = self.conv1d19.forward(add29_out1);
        let add30_out1 = conv1d19_out1.add(add27_out1);
        let mul43_out1 = add30_out1.clone().mul(constant61_out1);
        let sin22_out1 = mul43_out1.sin();
        let pow22_out1 = sin22_out1.powf_scalar(constant80_out1);
        let mul44_out1 = constant62_out1.mul(pow22_out1);
        let add31_out1 = add30_out1.add(mul44_out1);
        let convtranspose1d4_out1 = self.convtranspose1d4.forward(add31_out1);
        let mul45_out1 = convtranspose1d4_out1.clone().mul(constant63_out1.clone());
        let sin23_out1 = mul45_out1.sin();
        let pow23_out1 = sin23_out1.powf_scalar(constant80_out1);
        let mul46_out1 = constant64_out1.clone().mul(pow23_out1);
        let add32_out1 = convtranspose1d4_out1.clone().add(mul46_out1);
        let conv1d20_out1 = self.conv1d20.forward(add32_out1);
        let mul47_out1 = conv1d20_out1.clone().mul(constant63_out1);
        let sin24_out1 = mul47_out1.sin();
        let pow24_out1 = sin24_out1.powf_scalar(constant80_out1);
        let mul48_out1 = constant64_out1.mul(pow24_out1);
        let add33_out1 = conv1d20_out1.add(mul48_out1);
        let conv1d21_out1 = self.conv1d21.forward(add33_out1);
        let add34_out1 = conv1d21_out1.add(convtranspose1d4_out1);
        let mul49_out1 = add34_out1.clone().mul(constant65_out1.clone());
        let sin25_out1 = mul49_out1.sin();
        let pow25_out1 = sin25_out1.powf_scalar(constant80_out1);
        let mul50_out1 = constant66_out1.clone().mul(pow25_out1);
        let add35_out1 = add34_out1.clone().add(mul50_out1);
        let conv1d22_out1 = self.conv1d22.forward(add35_out1);
        let mul51_out1 = conv1d22_out1.clone().mul(constant65_out1);
        let sin26_out1 = mul51_out1.sin();
        let pow26_out1 = sin26_out1.powf_scalar(constant80_out1);
        let mul52_out1 = constant66_out1.mul(pow26_out1);
        let add36_out1 = conv1d22_out1.add(mul52_out1);
        let conv1d23_out1 = self.conv1d23.forward(add36_out1);
        let add37_out1 = conv1d23_out1.add(add34_out1);
        let mul53_out1 = add37_out1.clone().mul(constant67_out1.clone());
        let sin27_out1 = mul53_out1.sin();
        let pow27_out1 = sin27_out1.powf_scalar(constant80_out1);
        let mul54_out1 = constant68_out1.clone().mul(pow27_out1);
        let add38_out1 = add37_out1.clone().add(mul54_out1);
        let conv1d24_out1 = self.conv1d24.forward(add38_out1);
        let mul55_out1 = conv1d24_out1.clone().mul(constant67_out1);
        let sin28_out1 = mul55_out1.sin();
        let pow28_out1 = sin28_out1.powf_scalar(constant80_out1);
        let mul56_out1 = constant68_out1.mul(pow28_out1);
        let add39_out1 = conv1d24_out1.add(mul56_out1);
        let conv1d25_out1 = self.conv1d25.forward(add39_out1);
        let add40_out1 = conv1d25_out1.add(add37_out1);
        let mul57_out1 = add40_out1.clone().mul(constant69_out1);
        let sin29_out1 = mul57_out1.sin();
        let pow29_out1 = sin29_out1.powf_scalar(constant80_out1);
        let mul58_out1 = constant70_out1.mul(pow29_out1);
        let add41_out1 = add40_out1.add(mul58_out1);
        let convtranspose1d5_out1 = self.convtranspose1d5.forward(add41_out1);
        let mul59_out1 = convtranspose1d5_out1.clone().mul(constant71_out1.clone());
        let sin30_out1 = mul59_out1.sin();
        let pow30_out1 = sin30_out1.powf_scalar(constant80_out1);
        let mul60_out1 = constant72_out1.clone().mul(pow30_out1);
        let add42_out1 = convtranspose1d5_out1.clone().add(mul60_out1);
        let conv1d26_out1 = self.conv1d26.forward(add42_out1);
        let mul61_out1 = conv1d26_out1.clone().mul(constant71_out1);
        let sin31_out1 = mul61_out1.sin();
        let pow31_out1 = sin31_out1.powf_scalar(constant80_out1);
        let mul62_out1 = constant72_out1.mul(pow31_out1);
        let add43_out1 = conv1d26_out1.add(mul62_out1);
        let conv1d27_out1 = self.conv1d27.forward(add43_out1);
        let add44_out1 = conv1d27_out1.add(convtranspose1d5_out1);
        let mul63_out1 = add44_out1.clone().mul(constant73_out1.clone());
        let sin32_out1 = mul63_out1.sin();
        let pow32_out1 = sin32_out1.powf_scalar(constant80_out1);
        let mul64_out1 = constant74_out1.clone().mul(pow32_out1);
        let add45_out1 = add44_out1.clone().add(mul64_out1);
        let conv1d28_out1 = self.conv1d28.forward(add45_out1);
        let mul65_out1 = conv1d28_out1.clone().mul(constant73_out1);
        let sin33_out1 = mul65_out1.sin();
        let pow33_out1 = sin33_out1.powf_scalar(constant80_out1);
        let mul66_out1 = constant74_out1.mul(pow33_out1);
        let add46_out1 = conv1d28_out1.add(mul66_out1);
        let conv1d29_out1 = self.conv1d29.forward(add46_out1);
        let add47_out1 = conv1d29_out1.add(add44_out1);
        let mul67_out1 = add47_out1.clone().mul(constant75_out1.clone());
        let sin34_out1 = mul67_out1.sin();
        let pow34_out1 = sin34_out1.powf_scalar(constant80_out1);
        let mul68_out1 = constant76_out1.clone().mul(pow34_out1);
        let add48_out1 = add47_out1.clone().add(mul68_out1);
        let conv1d30_out1 = self.conv1d30.forward(add48_out1);
        let mul69_out1 = conv1d30_out1.clone().mul(constant75_out1);
        let sin35_out1 = mul69_out1.sin();
        let pow35_out1 = sin35_out1.powf_scalar(constant80_out1);
        let mul70_out1 = constant76_out1.mul(pow35_out1);
        let add49_out1 = conv1d30_out1.add(mul70_out1);
        let conv1d31_out1 = self.conv1d31.forward(add49_out1);
        let add50_out1 = conv1d31_out1.add(add47_out1);
        let mul71_out1 = add50_out1.clone().mul(constant77_out1);
        let sin36_out1 = mul71_out1.sin();
        let pow36_out1 = sin36_out1.powf_scalar(constant80_out1);
        let mul72_out1 = constant78_out1.mul(pow36_out1);
        let add51_out1 = add50_out1.add(mul72_out1);
        let conv1d32_out1 = self.conv1d32.forward(add51_out1);
        conv1d32_out1
    }
}
