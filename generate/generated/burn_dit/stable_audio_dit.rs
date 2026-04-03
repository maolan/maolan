// Generated from ONNX "stable_audio_dit.onnx" by burn-onnx
use burn::prelude::*;
use burn::nn::LayerNorm;
use burn::nn::LayerNormConfig;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::LinearLayout;
use burn::nn::PaddingConfig1d;
use burn::nn::conv::Conv1d;
use burn::nn::conv::Conv1dConfig;
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;


#[derive(Module, Debug)]
pub struct Model<B: Backend> {
    constant203: burn::module::Param<Tensor<B, 2>>,
    constant213: burn::module::Param<Tensor<B, 1>>,
    linear1: Linear<B>,
    linear2: Linear<B>,
    linear3: Linear<B>,
    linear4: Linear<B>,
    linear5: Linear<B>,
    conv1d1: Conv1d<B>,
    linear6: Linear<B>,
    layernormalization1: LayerNorm<B>,
    linear7: Linear<B>,
    linear8: Linear<B>,
    layernormalization2: LayerNorm<B>,
    linear9: Linear<B>,
    linear10: Linear<B>,
    linear11: Linear<B>,
    layernormalization3: LayerNorm<B>,
    linear12: Linear<B>,
    linear13: Linear<B>,
    layernormalization4: LayerNorm<B>,
    linear14: Linear<B>,
    linear15: Linear<B>,
    layernormalization5: LayerNorm<B>,
    linear16: Linear<B>,
    linear17: Linear<B>,
    linear18: Linear<B>,
    layernormalization6: LayerNorm<B>,
    linear19: Linear<B>,
    linear20: Linear<B>,
    layernormalization7: LayerNorm<B>,
    linear21: Linear<B>,
    linear22: Linear<B>,
    layernormalization8: LayerNorm<B>,
    linear23: Linear<B>,
    linear24: Linear<B>,
    linear25: Linear<B>,
    layernormalization9: LayerNorm<B>,
    linear26: Linear<B>,
    linear27: Linear<B>,
    layernormalization10: LayerNorm<B>,
    linear28: Linear<B>,
    linear29: Linear<B>,
    layernormalization11: LayerNorm<B>,
    linear30: Linear<B>,
    linear31: Linear<B>,
    linear32: Linear<B>,
    layernormalization12: LayerNorm<B>,
    linear33: Linear<B>,
    linear34: Linear<B>,
    layernormalization13: LayerNorm<B>,
    linear35: Linear<B>,
    linear36: Linear<B>,
    layernormalization14: LayerNorm<B>,
    linear37: Linear<B>,
    linear38: Linear<B>,
    linear39: Linear<B>,
    layernormalization15: LayerNorm<B>,
    linear40: Linear<B>,
    linear41: Linear<B>,
    layernormalization16: LayerNorm<B>,
    linear42: Linear<B>,
    linear43: Linear<B>,
    layernormalization17: LayerNorm<B>,
    linear44: Linear<B>,
    linear45: Linear<B>,
    linear46: Linear<B>,
    layernormalization18: LayerNorm<B>,
    linear47: Linear<B>,
    linear48: Linear<B>,
    layernormalization19: LayerNorm<B>,
    linear49: Linear<B>,
    linear50: Linear<B>,
    layernormalization20: LayerNorm<B>,
    linear51: Linear<B>,
    linear52: Linear<B>,
    linear53: Linear<B>,
    layernormalization21: LayerNorm<B>,
    linear54: Linear<B>,
    linear55: Linear<B>,
    layernormalization22: LayerNorm<B>,
    linear56: Linear<B>,
    linear57: Linear<B>,
    layernormalization23: LayerNorm<B>,
    linear58: Linear<B>,
    linear59: Linear<B>,
    linear60: Linear<B>,
    layernormalization24: LayerNorm<B>,
    linear61: Linear<B>,
    linear62: Linear<B>,
    layernormalization25: LayerNorm<B>,
    linear63: Linear<B>,
    linear64: Linear<B>,
    layernormalization26: LayerNorm<B>,
    linear65: Linear<B>,
    linear66: Linear<B>,
    linear67: Linear<B>,
    layernormalization27: LayerNorm<B>,
    linear68: Linear<B>,
    linear69: Linear<B>,
    layernormalization28: LayerNorm<B>,
    linear70: Linear<B>,
    linear71: Linear<B>,
    layernormalization29: LayerNorm<B>,
    linear72: Linear<B>,
    linear73: Linear<B>,
    linear74: Linear<B>,
    layernormalization30: LayerNorm<B>,
    linear75: Linear<B>,
    linear76: Linear<B>,
    layernormalization31: LayerNorm<B>,
    linear77: Linear<B>,
    linear78: Linear<B>,
    layernormalization32: LayerNorm<B>,
    linear79: Linear<B>,
    linear80: Linear<B>,
    linear81: Linear<B>,
    layernormalization33: LayerNorm<B>,
    linear82: Linear<B>,
    linear83: Linear<B>,
    layernormalization34: LayerNorm<B>,
    linear84: Linear<B>,
    linear85: Linear<B>,
    layernormalization35: LayerNorm<B>,
    linear86: Linear<B>,
    linear87: Linear<B>,
    linear88: Linear<B>,
    layernormalization36: LayerNorm<B>,
    linear89: Linear<B>,
    linear90: Linear<B>,
    layernormalization37: LayerNorm<B>,
    linear91: Linear<B>,
    linear92: Linear<B>,
    layernormalization38: LayerNorm<B>,
    linear93: Linear<B>,
    linear94: Linear<B>,
    linear95: Linear<B>,
    layernormalization39: LayerNorm<B>,
    linear96: Linear<B>,
    linear97: Linear<B>,
    layernormalization40: LayerNorm<B>,
    linear98: Linear<B>,
    linear99: Linear<B>,
    layernormalization41: LayerNorm<B>,
    linear100: Linear<B>,
    linear101: Linear<B>,
    linear102: Linear<B>,
    layernormalization42: LayerNorm<B>,
    linear103: Linear<B>,
    linear104: Linear<B>,
    layernormalization43: LayerNorm<B>,
    linear105: Linear<B>,
    linear106: Linear<B>,
    layernormalization44: LayerNorm<B>,
    linear107: Linear<B>,
    linear108: Linear<B>,
    linear109: Linear<B>,
    layernormalization45: LayerNorm<B>,
    linear110: Linear<B>,
    linear111: Linear<B>,
    layernormalization46: LayerNorm<B>,
    linear112: Linear<B>,
    linear113: Linear<B>,
    layernormalization47: LayerNorm<B>,
    linear114: Linear<B>,
    linear115: Linear<B>,
    linear116: Linear<B>,
    layernormalization48: LayerNorm<B>,
    linear117: Linear<B>,
    linear118: Linear<B>,
    layernormalization49: LayerNorm<B>,
    linear119: Linear<B>,
    linear120: Linear<B>,
    layernormalization50: LayerNorm<B>,
    linear121: Linear<B>,
    linear122: Linear<B>,
    linear123: Linear<B>,
    layernormalization51: LayerNorm<B>,
    linear124: Linear<B>,
    linear125: Linear<B>,
    layernormalization52: LayerNorm<B>,
    linear126: Linear<B>,
    linear127: Linear<B>,
    layernormalization53: LayerNorm<B>,
    linear128: Linear<B>,
    linear129: Linear<B>,
    linear130: Linear<B>,
    layernormalization54: LayerNorm<B>,
    linear131: Linear<B>,
    linear132: Linear<B>,
    layernormalization55: LayerNorm<B>,
    linear133: Linear<B>,
    linear134: Linear<B>,
    layernormalization56: LayerNorm<B>,
    linear135: Linear<B>,
    linear136: Linear<B>,
    linear137: Linear<B>,
    layernormalization57: LayerNorm<B>,
    linear138: Linear<B>,
    linear139: Linear<B>,
    layernormalization58: LayerNorm<B>,
    linear140: Linear<B>,
    linear141: Linear<B>,
    layernormalization59: LayerNorm<B>,
    linear142: Linear<B>,
    linear143: Linear<B>,
    linear144: Linear<B>,
    layernormalization60: LayerNorm<B>,
    linear145: Linear<B>,
    linear146: Linear<B>,
    layernormalization61: LayerNorm<B>,
    linear147: Linear<B>,
    linear148: Linear<B>,
    layernormalization62: LayerNorm<B>,
    linear149: Linear<B>,
    linear150: Linear<B>,
    linear151: Linear<B>,
    layernormalization63: LayerNorm<B>,
    linear152: Linear<B>,
    linear153: Linear<B>,
    layernormalization64: LayerNorm<B>,
    linear154: Linear<B>,
    linear155: Linear<B>,
    layernormalization65: LayerNorm<B>,
    linear156: Linear<B>,
    linear157: Linear<B>,
    linear158: Linear<B>,
    layernormalization66: LayerNorm<B>,
    linear159: Linear<B>,
    linear160: Linear<B>,
    layernormalization67: LayerNorm<B>,
    linear161: Linear<B>,
    linear162: Linear<B>,
    layernormalization68: LayerNorm<B>,
    linear163: Linear<B>,
    linear164: Linear<B>,
    linear165: Linear<B>,
    layernormalization69: LayerNorm<B>,
    linear166: Linear<B>,
    linear167: Linear<B>,
    layernormalization70: LayerNorm<B>,
    linear168: Linear<B>,
    linear169: Linear<B>,
    layernormalization71: LayerNorm<B>,
    linear170: Linear<B>,
    linear171: Linear<B>,
    linear172: Linear<B>,
    layernormalization72: LayerNorm<B>,
    linear173: Linear<B>,
    linear174: Linear<B>,
    linear175: Linear<B>,
    conv1d2: Conv1d<B>,
    phantom: core::marker::PhantomData<B>,
    device: burn::module::Ignored<B::Device>,
}


impl<B: Backend> Default for Model<B> {
    fn default() -> Self {
        Self::from_file("burn_dit/stable_audio_dit.bpk", &Default::default())
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
        let constant203: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 2>::zeros([257, 32], device),
            device.clone(),
            false,
            [257, 32].into(),
        );
        let constant213: burn::module::Param<Tensor<B, 1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<B, 1>::zeros([1], device),
            device.clone(),
            false,
            [1].into(),
        );
        let linear1 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear2 = LinearConfig::new(768, 768).with_bias(false).init(device);
        let linear3 = LinearConfig::new(1, 128).with_bias(false).init(device);
        let linear4 = LinearConfig::new(256, 1536)
            .with_bias(true)
            .with_layout(LinearLayout::Col)
            .init(device);
        let linear5 = LinearConfig::new(1536, 1536)
            .with_bias(true)
            .with_layout(LinearLayout::Col)
            .init(device);
        let conv1d1 = Conv1dConfig::new(64, 64, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(false)
            .init(device);
        let linear6 = LinearConfig::new(64, 1536).with_bias(false).init(device);
        let layernormalization1 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear7 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear8 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization2 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear9 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear10 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear11 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization3 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear12 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear13 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization4 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear14 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear15 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization5 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear16 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear17 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear18 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization6 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear19 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear20 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization7 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear21 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear22 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization8 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear23 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear24 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear25 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization9 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear26 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear27 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization10 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear28 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear29 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization11 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear30 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear31 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear32 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization12 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear33 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear34 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization13 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear35 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear36 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization14 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear37 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear38 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear39 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization15 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear40 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear41 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization16 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear42 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear43 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization17 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear44 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear45 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear46 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization18 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear47 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear48 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization19 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear49 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear50 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization20 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear51 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear52 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear53 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization21 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear54 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear55 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization22 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear56 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear57 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization23 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear58 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear59 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear60 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization24 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear61 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear62 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization25 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear63 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear64 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization26 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear65 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear66 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear67 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization27 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear68 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear69 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization28 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear70 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear71 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization29 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear72 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear73 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear74 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization30 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear75 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear76 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization31 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear77 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear78 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization32 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear79 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear80 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear81 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization33 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear82 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear83 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization34 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear84 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear85 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization35 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear86 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear87 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear88 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization36 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear89 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear90 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization37 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear91 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear92 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization38 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear93 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear94 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear95 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization39 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear96 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear97 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization40 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear98 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear99 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization41 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear100 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear101 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear102 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization42 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear103 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear104 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization43 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear105 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear106 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization44 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear107 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear108 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear109 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization45 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear110 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear111 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization46 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear112 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear113 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization47 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear114 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear115 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear116 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization48 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear117 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear118 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization49 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear119 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear120 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization50 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear121 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear122 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear123 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization51 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear124 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear125 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization52 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear126 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear127 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization53 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear128 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear129 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear130 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization54 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear131 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear132 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization55 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear133 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear134 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization56 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear135 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear136 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear137 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization57 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear138 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear139 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization58 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear140 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear141 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization59 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear142 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear143 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear144 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization60 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear145 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear146 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization61 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear147 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear148 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization62 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear149 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear150 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear151 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization63 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear152 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear153 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization64 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear154 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear155 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization65 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear156 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear157 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear158 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization66 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear159 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear160 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization67 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear161 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear162 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization68 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear163 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear164 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear165 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization69 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear166 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear167 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let layernormalization70 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear168 = LinearConfig::new(1536, 4608).with_bias(false).init(device);
        let linear169 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization71 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear170 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let linear171 = LinearConfig::new(768, 1536).with_bias(false).init(device);
        let linear172 = LinearConfig::new(1536, 1536).with_bias(false).init(device);
        let layernormalization72 = LayerNormConfig::new(1536)
            .with_epsilon(0.000009999999747378752f64)
            .with_bias(true)
            .init(device);
        let linear173 = LinearConfig::new(1536, 12288).with_bias(true).init(device);
        let linear174 = LinearConfig::new(6144, 1536).with_bias(true).init(device);
        let linear175 = LinearConfig::new(1536, 64).with_bias(false).init(device);
        let conv1d2 = Conv1dConfig::new(64, 64, 1)
            .with_stride(1)
            .with_padding(PaddingConfig1d::Valid)
            .with_dilation(1)
            .with_groups(1)
            .with_bias(false)
            .init(device);
        Self {
            constant203,
            constant213,
            linear1,
            linear2,
            linear3,
            linear4,
            linear5,
            conv1d1,
            linear6,
            layernormalization1,
            linear7,
            linear8,
            layernormalization2,
            linear9,
            linear10,
            linear11,
            layernormalization3,
            linear12,
            linear13,
            layernormalization4,
            linear14,
            linear15,
            layernormalization5,
            linear16,
            linear17,
            linear18,
            layernormalization6,
            linear19,
            linear20,
            layernormalization7,
            linear21,
            linear22,
            layernormalization8,
            linear23,
            linear24,
            linear25,
            layernormalization9,
            linear26,
            linear27,
            layernormalization10,
            linear28,
            linear29,
            layernormalization11,
            linear30,
            linear31,
            linear32,
            layernormalization12,
            linear33,
            linear34,
            layernormalization13,
            linear35,
            linear36,
            layernormalization14,
            linear37,
            linear38,
            linear39,
            layernormalization15,
            linear40,
            linear41,
            layernormalization16,
            linear42,
            linear43,
            layernormalization17,
            linear44,
            linear45,
            linear46,
            layernormalization18,
            linear47,
            linear48,
            layernormalization19,
            linear49,
            linear50,
            layernormalization20,
            linear51,
            linear52,
            linear53,
            layernormalization21,
            linear54,
            linear55,
            layernormalization22,
            linear56,
            linear57,
            layernormalization23,
            linear58,
            linear59,
            linear60,
            layernormalization24,
            linear61,
            linear62,
            layernormalization25,
            linear63,
            linear64,
            layernormalization26,
            linear65,
            linear66,
            linear67,
            layernormalization27,
            linear68,
            linear69,
            layernormalization28,
            linear70,
            linear71,
            layernormalization29,
            linear72,
            linear73,
            linear74,
            layernormalization30,
            linear75,
            linear76,
            layernormalization31,
            linear77,
            linear78,
            layernormalization32,
            linear79,
            linear80,
            linear81,
            layernormalization33,
            linear82,
            linear83,
            layernormalization34,
            linear84,
            linear85,
            layernormalization35,
            linear86,
            linear87,
            linear88,
            layernormalization36,
            linear89,
            linear90,
            layernormalization37,
            linear91,
            linear92,
            layernormalization38,
            linear93,
            linear94,
            linear95,
            layernormalization39,
            linear96,
            linear97,
            layernormalization40,
            linear98,
            linear99,
            layernormalization41,
            linear100,
            linear101,
            linear102,
            layernormalization42,
            linear103,
            linear104,
            layernormalization43,
            linear105,
            linear106,
            layernormalization44,
            linear107,
            linear108,
            linear109,
            layernormalization45,
            linear110,
            linear111,
            layernormalization46,
            linear112,
            linear113,
            layernormalization47,
            linear114,
            linear115,
            linear116,
            layernormalization48,
            linear117,
            linear118,
            layernormalization49,
            linear119,
            linear120,
            layernormalization50,
            linear121,
            linear122,
            linear123,
            layernormalization51,
            linear124,
            linear125,
            layernormalization52,
            linear126,
            linear127,
            layernormalization53,
            linear128,
            linear129,
            linear130,
            layernormalization54,
            linear131,
            linear132,
            layernormalization55,
            linear133,
            linear134,
            layernormalization56,
            linear135,
            linear136,
            linear137,
            layernormalization57,
            linear138,
            linear139,
            layernormalization58,
            linear140,
            linear141,
            layernormalization59,
            linear142,
            linear143,
            linear144,
            layernormalization60,
            linear145,
            linear146,
            layernormalization61,
            linear147,
            linear148,
            layernormalization62,
            linear149,
            linear150,
            linear151,
            layernormalization63,
            linear152,
            linear153,
            layernormalization64,
            linear154,
            linear155,
            layernormalization65,
            linear156,
            linear157,
            linear158,
            layernormalization66,
            linear159,
            linear160,
            layernormalization67,
            linear161,
            linear162,
            layernormalization68,
            linear163,
            linear164,
            linear165,
            layernormalization69,
            linear166,
            linear167,
            layernormalization70,
            linear168,
            linear169,
            layernormalization71,
            linear170,
            linear171,
            linear172,
            layernormalization72,
            linear173,
            linear174,
            linear175,
            conv1d2,
            phantom: core::marker::PhantomData,
            device: burn::module::Ignored(device.clone()),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(
        &self,
        latent_input: Tensor<B, 3>,
        timestep: Tensor<B, 1>,
        text_embedding: Tensor<B, 3>,
    ) -> Tensor<B, 3> {
        let constant203_out1 = self.constant203.val();
        let constant213_out1 = self.constant213.val();
        let constant389_out1 = 6.2831855f32;
        let linear1_out1 = self.linear1.forward(text_embedding);
        let sigmoid1_out1 = burn::tensor::activation::sigmoid(linear1_out1.clone());
        let mul1_out1 = linear1_out1.mul(sigmoid1_out1);
        let linear2_out1 = self.linear2.forward(mul1_out1);
        let unsqueeze1_out1: Tensor<B, 2> = timestep.unsqueeze_dims::<2>(&[1]);
        let mul2_out1 = unsqueeze1_out1.mul_scalar(constant389_out1);
        let linear3_out1 = self.linear3.forward(mul2_out1);
        let cos1_out1 = linear3_out1.clone().cos();
        let sin1_out1 = linear3_out1.sin();
        let concat1_out1 = burn::tensor::Tensor::cat([cos1_out1, sin1_out1].into(), 1);
        let linear4_out1 = self.linear4.forward(concat1_out1);
        let sigmoid2_out1 = burn::tensor::activation::sigmoid(linear4_out1.clone());
        let mul3_out1 = linear4_out1.mul(sigmoid2_out1);
        let linear5_out1 = self.linear5.forward(mul3_out1);
        let unsqueeze2_out1: Tensor<B, 3> = linear5_out1.unsqueeze_dims::<3>(&[1]);
        let conv1d1_out1 = self.conv1d1.forward(latent_input.clone());
        let add1_out1 = conv1d1_out1.add(latent_input);
        let transpose1_out1 = add1_out1.permute([0, 2, 1]);
        let linear6_out1 = self.linear6.forward(transpose1_out1);
        let concat2_out1 = burn::tensor::Tensor::cat(
            [unsqueeze2_out1, linear6_out1].into(),
            1,
        );
        let layernormalization1_out1 = {
            let dtype = concat2_out1.clone().dtype();
            self.layernormalization1
                .forward(concat2_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear7_out1 = self.linear7.forward(layernormalization1_out1);
        let split_tensors = linear7_out1.split(1536, 2);
        let [split1_out1, split1_out2, split1_out3] = split_tensors.try_into().unwrap();
        let reshape1_out1 = split1_out1.reshape([1, 257, 24, 64]);
        let transpose2_out1 = reshape1_out1.permute([0, 2, 1, 3]);
        let reshape2_out1 = split1_out2.reshape([1, 257, 24, 64]);
        let transpose3_out1 = reshape2_out1.permute([0, 2, 1, 3]);
        let reshape3_out1 = split1_out3.reshape([1, 257, 24, 64]);
        let transpose4_out1 = reshape3_out1.permute([0, 2, 1, 3]);
        let slice1_out1 = transpose2_out1.clone().slice(s![.., .., .., 0..32]);
        let slice2_out1 = transpose2_out1.slice(s![.., .., .., 32..]);
        let cos2_out1 = constant203_out1.clone().cos();
        let mul4_out1 = slice1_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape4_out1 = slice1_out1.reshape([1, 24, 257, 2, 16]);
        let slice3_out1 = reshape4_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze1_out1 = slice3_out1.squeeze_dims::<4>(&[-2]);
        let slice4_out1 = reshape4_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze2_out1 = slice4_out1.squeeze_dims::<4>(&[-2]);
        let neg1_out1 = squeeze2_out1.neg();
        let concat3_out1 = burn::tensor::Tensor::cat(
            [neg1_out1, squeeze1_out1].into(),
            3,
        );
        let sin2_out1 = constant203_out1.sin();
        let mul5_out1 = concat3_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add2_out1 = mul4_out1.add(mul5_out1);
        let concat4_out1 = burn::tensor::Tensor::cat([add2_out1, slice2_out1].into(), 3);
        let slice5_out1 = transpose3_out1.clone().slice(s![.., .., .., 0..32]);
        let slice6_out1 = transpose3_out1.slice(s![.., .., .., 32..]);
        let mul6_out1 = slice5_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape5_out1 = slice5_out1.reshape([1, 24, 257, 2, 16]);
        let slice7_out1 = reshape5_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze3_out1 = slice7_out1.squeeze_dims::<4>(&[-2]);
        let slice8_out1 = reshape5_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze4_out1 = slice8_out1.squeeze_dims::<4>(&[-2]);
        let neg2_out1 = squeeze4_out1.neg();
        let concat5_out1 = burn::tensor::Tensor::cat(
            [neg2_out1, squeeze3_out1].into(),
            3,
        );
        let mul7_out1 = concat5_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add3_out1 = mul6_out1.add(mul7_out1);
        let concat6_out1 = burn::tensor::Tensor::cat([add3_out1, slice6_out1].into(), 3);
        let reshape6_out1 = concat6_out1.reshape([-1, 257, 64]);
        let transpose5_out1 = reshape6_out1.permute([0, 2, 1]);
        let reshape7_out1 = transpose5_out1.reshape([1, 24, 64, 257]);
        let mul8_out1 = concat4_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul9_out1 = reshape7_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul6_out1 = mul8_out1.matmul(mul9_out1);
        let softmax1_out1 = burn::tensor::activation::softmax(matmul6_out1, 3);
        let matmul7_out1 = softmax1_out1.matmul(transpose4_out1);
        let transpose6_out1 = matmul7_out1.permute([0, 2, 1, 3]);
        let reshape8_out1 = transpose6_out1.reshape([1, 257, 1536]);
        let linear8_out1 = self.linear8.forward(reshape8_out1);
        let add4_out1 = concat2_out1.add(linear8_out1);
        let layernormalization2_out1 = {
            let dtype = add4_out1.clone().dtype();
            self.layernormalization2
                .forward(add4_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear9_out1 = self.linear9.forward(layernormalization2_out1);
        let reshape9_out1 = linear9_out1.reshape([1, 257, 24, 64]);
        let transpose7_out1 = reshape9_out1.permute([0, 2, 1, 3]);
        let linear10_out1 = self.linear10.forward(linear2_out1.clone());
        let split_tensors = linear10_out1.split(768, 2);
        let [split2_out1, split2_out2] = split_tensors.try_into().unwrap();
        let reshape10_out1 = split2_out1.reshape([1, 130, 12, 64]);
        let transpose8_out1 = reshape10_out1.permute([0, 2, 1, 3]);
        let reshape11_out1 = split2_out2.reshape([1, 130, 12, 64]);
        let transpose9_out1 = reshape11_out1.permute([0, 2, 1, 3]);
        let unsqueeze3_out1: Tensor<B, 5> = transpose8_out1.unsqueeze_dims::<5>(&[2]);
        let expand1_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze3_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze3_out1.expand(shape)
        };
        let unsqueeze4_out1: Tensor<B, 5> = transpose9_out1.unsqueeze_dims::<5>(&[2]);
        let expand2_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze4_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze4_out1.expand(shape)
        };
        let reshape12_out1 = expand2_out1.reshape([1, -1, 130, 64]);
        let reshape13_out1 = expand1_out1.reshape([24, 130, 64]);
        let transpose10_out1 = reshape13_out1.permute([0, 2, 1]);
        let reshape14_out1 = transpose10_out1.reshape([1, 24, 64, 130]);
        let mul10_out1 = transpose7_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul11_out1 = reshape14_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul11_out1 = mul10_out1.matmul(mul11_out1);
        let softmax2_out1 = burn::tensor::activation::softmax(matmul11_out1, 3);
        let matmul12_out1 = softmax2_out1.matmul(reshape12_out1);
        let transpose11_out1 = matmul12_out1.permute([0, 2, 1, 3]);
        let reshape15_out1 = transpose11_out1.reshape([1, 257, 1536]);
        let linear11_out1 = self.linear11.forward(reshape15_out1);
        let add5_out1 = add4_out1.add(linear11_out1);
        let layernormalization3_out1 = {
            let dtype = add5_out1.clone().dtype();
            self.layernormalization3
                .forward(add5_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear12_out1 = self.linear12.forward(layernormalization3_out1);
        let split_tensors = linear12_out1.split(6144, 2);
        let [split3_out1, split3_out2] = split_tensors.try_into().unwrap();
        let sigmoid3_out1 = burn::tensor::activation::sigmoid(split3_out2.clone());
        let mul12_out1 = split3_out2.mul(sigmoid3_out1);
        let mul13_out1 = split3_out1.mul(mul12_out1);
        let linear13_out1 = self.linear13.forward(mul13_out1);
        let add6_out1 = add5_out1.add(linear13_out1);
        let layernormalization4_out1 = {
            let dtype = add6_out1.clone().dtype();
            self.layernormalization4
                .forward(add6_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear14_out1 = self.linear14.forward(layernormalization4_out1);
        let split_tensors = linear14_out1.split(1536, 2);
        let [split4_out1, split4_out2, split4_out3] = split_tensors.try_into().unwrap();
        let reshape16_out1 = split4_out1.reshape([1, 257, 24, 64]);
        let transpose12_out1 = reshape16_out1.permute([0, 2, 1, 3]);
        let reshape17_out1 = split4_out2.reshape([1, 257, 24, 64]);
        let transpose13_out1 = reshape17_out1.permute([0, 2, 1, 3]);
        let reshape18_out1 = split4_out3.reshape([1, 257, 24, 64]);
        let transpose14_out1 = reshape18_out1.permute([0, 2, 1, 3]);
        let slice9_out1 = transpose12_out1.clone().slice(s![.., .., .., 0..32]);
        let slice10_out1 = transpose12_out1.slice(s![.., .., .., 32..]);
        let mul14_out1 = slice9_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape19_out1 = slice9_out1.reshape([1, 24, 257, 2, 16]);
        let slice11_out1 = reshape19_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze5_out1 = slice11_out1.squeeze_dims::<4>(&[-2]);
        let slice12_out1 = reshape19_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze6_out1 = slice12_out1.squeeze_dims::<4>(&[-2]);
        let neg3_out1 = squeeze6_out1.neg();
        let concat7_out1 = burn::tensor::Tensor::cat(
            [neg3_out1, squeeze5_out1].into(),
            3,
        );
        let mul15_out1 = concat7_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add7_out1 = mul14_out1.add(mul15_out1);
        let concat8_out1 = burn::tensor::Tensor::cat(
            [add7_out1, slice10_out1].into(),
            3,
        );
        let slice13_out1 = transpose13_out1.clone().slice(s![.., .., .., 0..32]);
        let slice14_out1 = transpose13_out1.slice(s![.., .., .., 32..]);
        let mul16_out1 = slice13_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape20_out1 = slice13_out1.reshape([1, 24, 257, 2, 16]);
        let slice15_out1 = reshape20_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze7_out1 = slice15_out1.squeeze_dims::<4>(&[-2]);
        let slice16_out1 = reshape20_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze8_out1 = slice16_out1.squeeze_dims::<4>(&[-2]);
        let neg4_out1 = squeeze8_out1.neg();
        let concat9_out1 = burn::tensor::Tensor::cat(
            [neg4_out1, squeeze7_out1].into(),
            3,
        );
        let mul17_out1 = concat9_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add8_out1 = mul16_out1.add(mul17_out1);
        let concat10_out1 = burn::tensor::Tensor::cat(
            [add8_out1, slice14_out1].into(),
            3,
        );
        let reshape21_out1 = concat10_out1.reshape([-1, 257, 64]);
        let transpose15_out1 = reshape21_out1.permute([0, 2, 1]);
        let reshape22_out1 = transpose15_out1.reshape([1, 24, 64, 257]);
        let mul18_out1 = concat8_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul19_out1 = reshape22_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul17_out1 = mul18_out1.matmul(mul19_out1);
        let softmax3_out1 = burn::tensor::activation::softmax(matmul17_out1, 3);
        let matmul18_out1 = softmax3_out1.matmul(transpose14_out1);
        let transpose16_out1 = matmul18_out1.permute([0, 2, 1, 3]);
        let reshape23_out1 = transpose16_out1.reshape([1, 257, 1536]);
        let linear15_out1 = self.linear15.forward(reshape23_out1);
        let add9_out1 = add6_out1.add(linear15_out1);
        let layernormalization5_out1 = {
            let dtype = add9_out1.clone().dtype();
            self.layernormalization5
                .forward(add9_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear16_out1 = self.linear16.forward(layernormalization5_out1);
        let reshape24_out1 = linear16_out1.reshape([1, 257, 24, 64]);
        let transpose17_out1 = reshape24_out1.permute([0, 2, 1, 3]);
        let linear17_out1 = self.linear17.forward(linear2_out1.clone());
        let split_tensors = linear17_out1.split(768, 2);
        let [split5_out1, split5_out2] = split_tensors.try_into().unwrap();
        let reshape25_out1 = split5_out1.reshape([1, 130, 12, 64]);
        let transpose18_out1 = reshape25_out1.permute([0, 2, 1, 3]);
        let reshape26_out1 = split5_out2.reshape([1, 130, 12, 64]);
        let transpose19_out1 = reshape26_out1.permute([0, 2, 1, 3]);
        let unsqueeze5_out1: Tensor<B, 5> = transpose18_out1.unsqueeze_dims::<5>(&[2]);
        let expand3_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze5_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze5_out1.expand(shape)
        };
        let unsqueeze6_out1: Tensor<B, 5> = transpose19_out1.unsqueeze_dims::<5>(&[2]);
        let expand4_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze6_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze6_out1.expand(shape)
        };
        let reshape27_out1 = expand4_out1.reshape([1, -1, 130, 64]);
        let reshape28_out1 = expand3_out1.reshape([24, 130, 64]);
        let transpose20_out1 = reshape28_out1.permute([0, 2, 1]);
        let reshape29_out1 = transpose20_out1.reshape([1, 24, 64, 130]);
        let mul20_out1 = transpose17_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul21_out1 = reshape29_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul22_out1 = mul20_out1.matmul(mul21_out1);
        let softmax4_out1 = burn::tensor::activation::softmax(matmul22_out1, 3);
        let matmul23_out1 = softmax4_out1.matmul(reshape27_out1);
        let transpose21_out1 = matmul23_out1.permute([0, 2, 1, 3]);
        let reshape30_out1 = transpose21_out1.reshape([1, 257, 1536]);
        let linear18_out1 = self.linear18.forward(reshape30_out1);
        let add10_out1 = add9_out1.add(linear18_out1);
        let layernormalization6_out1 = {
            let dtype = add10_out1.clone().dtype();
            self.layernormalization6
                .forward(add10_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear19_out1 = self.linear19.forward(layernormalization6_out1);
        let split_tensors = linear19_out1.split(6144, 2);
        let [split6_out1, split6_out2] = split_tensors.try_into().unwrap();
        let sigmoid4_out1 = burn::tensor::activation::sigmoid(split6_out2.clone());
        let mul22_out1 = split6_out2.mul(sigmoid4_out1);
        let mul23_out1 = split6_out1.mul(mul22_out1);
        let linear20_out1 = self.linear20.forward(mul23_out1);
        let add11_out1 = add10_out1.add(linear20_out1);
        let layernormalization7_out1 = {
            let dtype = add11_out1.clone().dtype();
            self.layernormalization7
                .forward(add11_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear21_out1 = self.linear21.forward(layernormalization7_out1);
        let split_tensors = linear21_out1.split(1536, 2);
        let [split7_out1, split7_out2, split7_out3] = split_tensors.try_into().unwrap();
        let reshape31_out1 = split7_out1.reshape([1, 257, 24, 64]);
        let transpose22_out1 = reshape31_out1.permute([0, 2, 1, 3]);
        let reshape32_out1 = split7_out2.reshape([1, 257, 24, 64]);
        let transpose23_out1 = reshape32_out1.permute([0, 2, 1, 3]);
        let reshape33_out1 = split7_out3.reshape([1, 257, 24, 64]);
        let transpose24_out1 = reshape33_out1.permute([0, 2, 1, 3]);
        let slice17_out1 = transpose22_out1.clone().slice(s![.., .., .., 0..32]);
        let slice18_out1 = transpose22_out1.slice(s![.., .., .., 32..]);
        let mul24_out1 = slice17_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape34_out1 = slice17_out1.reshape([1, 24, 257, 2, 16]);
        let slice19_out1 = reshape34_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze9_out1 = slice19_out1.squeeze_dims::<4>(&[-2]);
        let slice20_out1 = reshape34_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze10_out1 = slice20_out1.squeeze_dims::<4>(&[-2]);
        let neg5_out1 = squeeze10_out1.neg();
        let concat11_out1 = burn::tensor::Tensor::cat(
            [neg5_out1, squeeze9_out1].into(),
            3,
        );
        let mul25_out1 = concat11_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add12_out1 = mul24_out1.add(mul25_out1);
        let concat12_out1 = burn::tensor::Tensor::cat(
            [add12_out1, slice18_out1].into(),
            3,
        );
        let slice21_out1 = transpose23_out1.clone().slice(s![.., .., .., 0..32]);
        let slice22_out1 = transpose23_out1.slice(s![.., .., .., 32..]);
        let mul26_out1 = slice21_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape35_out1 = slice21_out1.reshape([1, 24, 257, 2, 16]);
        let slice23_out1 = reshape35_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze11_out1 = slice23_out1.squeeze_dims::<4>(&[-2]);
        let slice24_out1 = reshape35_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze12_out1 = slice24_out1.squeeze_dims::<4>(&[-2]);
        let neg6_out1 = squeeze12_out1.neg();
        let concat13_out1 = burn::tensor::Tensor::cat(
            [neg6_out1, squeeze11_out1].into(),
            3,
        );
        let mul27_out1 = concat13_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add13_out1 = mul26_out1.add(mul27_out1);
        let concat14_out1 = burn::tensor::Tensor::cat(
            [add13_out1, slice22_out1].into(),
            3,
        );
        let reshape36_out1 = concat14_out1.reshape([-1, 257, 64]);
        let transpose25_out1 = reshape36_out1.permute([0, 2, 1]);
        let reshape37_out1 = transpose25_out1.reshape([1, 24, 64, 257]);
        let mul28_out1 = concat12_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul29_out1 = reshape37_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul28_out1 = mul28_out1.matmul(mul29_out1);
        let softmax5_out1 = burn::tensor::activation::softmax(matmul28_out1, 3);
        let matmul29_out1 = softmax5_out1.matmul(transpose24_out1);
        let transpose26_out1 = matmul29_out1.permute([0, 2, 1, 3]);
        let reshape38_out1 = transpose26_out1.reshape([1, 257, 1536]);
        let linear22_out1 = self.linear22.forward(reshape38_out1);
        let add14_out1 = add11_out1.add(linear22_out1);
        let layernormalization8_out1 = {
            let dtype = add14_out1.clone().dtype();
            self.layernormalization8
                .forward(add14_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear23_out1 = self.linear23.forward(layernormalization8_out1);
        let reshape39_out1 = linear23_out1.reshape([1, 257, 24, 64]);
        let transpose27_out1 = reshape39_out1.permute([0, 2, 1, 3]);
        let linear24_out1 = self.linear24.forward(linear2_out1.clone());
        let split_tensors = linear24_out1.split(768, 2);
        let [split8_out1, split8_out2] = split_tensors.try_into().unwrap();
        let reshape40_out1 = split8_out1.reshape([1, 130, 12, 64]);
        let transpose28_out1 = reshape40_out1.permute([0, 2, 1, 3]);
        let reshape41_out1 = split8_out2.reshape([1, 130, 12, 64]);
        let transpose29_out1 = reshape41_out1.permute([0, 2, 1, 3]);
        let unsqueeze7_out1: Tensor<B, 5> = transpose28_out1.unsqueeze_dims::<5>(&[2]);
        let expand5_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze7_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze7_out1.expand(shape)
        };
        let unsqueeze8_out1: Tensor<B, 5> = transpose29_out1.unsqueeze_dims::<5>(&[2]);
        let expand6_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze8_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze8_out1.expand(shape)
        };
        let reshape42_out1 = expand6_out1.reshape([1, -1, 130, 64]);
        let reshape43_out1 = expand5_out1.reshape([24, 130, 64]);
        let transpose30_out1 = reshape43_out1.permute([0, 2, 1]);
        let reshape44_out1 = transpose30_out1.reshape([1, 24, 64, 130]);
        let mul30_out1 = transpose27_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul31_out1 = reshape44_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul33_out1 = mul30_out1.matmul(mul31_out1);
        let softmax6_out1 = burn::tensor::activation::softmax(matmul33_out1, 3);
        let matmul34_out1 = softmax6_out1.matmul(reshape42_out1);
        let transpose31_out1 = matmul34_out1.permute([0, 2, 1, 3]);
        let reshape45_out1 = transpose31_out1.reshape([1, 257, 1536]);
        let linear25_out1 = self.linear25.forward(reshape45_out1);
        let add15_out1 = add14_out1.add(linear25_out1);
        let layernormalization9_out1 = {
            let dtype = add15_out1.clone().dtype();
            self.layernormalization9
                .forward(add15_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear26_out1 = self.linear26.forward(layernormalization9_out1);
        let split_tensors = linear26_out1.split(6144, 2);
        let [split9_out1, split9_out2] = split_tensors.try_into().unwrap();
        let sigmoid5_out1 = burn::tensor::activation::sigmoid(split9_out2.clone());
        let mul32_out1 = split9_out2.mul(sigmoid5_out1);
        let mul33_out1 = split9_out1.mul(mul32_out1);
        let linear27_out1 = self.linear27.forward(mul33_out1);
        let add16_out1 = add15_out1.add(linear27_out1);
        let layernormalization10_out1 = {
            let dtype = add16_out1.clone().dtype();
            self.layernormalization10
                .forward(add16_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear28_out1 = self.linear28.forward(layernormalization10_out1);
        let split_tensors = linear28_out1.split(1536, 2);
        let [split10_out1, split10_out2, split10_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape46_out1 = split10_out1.reshape([1, 257, 24, 64]);
        let transpose32_out1 = reshape46_out1.permute([0, 2, 1, 3]);
        let reshape47_out1 = split10_out2.reshape([1, 257, 24, 64]);
        let transpose33_out1 = reshape47_out1.permute([0, 2, 1, 3]);
        let reshape48_out1 = split10_out3.reshape([1, 257, 24, 64]);
        let transpose34_out1 = reshape48_out1.permute([0, 2, 1, 3]);
        let slice25_out1 = transpose32_out1.clone().slice(s![.., .., .., 0..32]);
        let slice26_out1 = transpose32_out1.slice(s![.., .., .., 32..]);
        let mul34_out1 = slice25_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape49_out1 = slice25_out1.reshape([1, 24, 257, 2, 16]);
        let slice27_out1 = reshape49_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze13_out1 = slice27_out1.squeeze_dims::<4>(&[-2]);
        let slice28_out1 = reshape49_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze14_out1 = slice28_out1.squeeze_dims::<4>(&[-2]);
        let neg7_out1 = squeeze14_out1.neg();
        let concat15_out1 = burn::tensor::Tensor::cat(
            [neg7_out1, squeeze13_out1].into(),
            3,
        );
        let mul35_out1 = concat15_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add17_out1 = mul34_out1.add(mul35_out1);
        let concat16_out1 = burn::tensor::Tensor::cat(
            [add17_out1, slice26_out1].into(),
            3,
        );
        let slice29_out1 = transpose33_out1.clone().slice(s![.., .., .., 0..32]);
        let slice30_out1 = transpose33_out1.slice(s![.., .., .., 32..]);
        let mul36_out1 = slice29_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape50_out1 = slice29_out1.reshape([1, 24, 257, 2, 16]);
        let slice31_out1 = reshape50_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze15_out1 = slice31_out1.squeeze_dims::<4>(&[-2]);
        let slice32_out1 = reshape50_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze16_out1 = slice32_out1.squeeze_dims::<4>(&[-2]);
        let neg8_out1 = squeeze16_out1.neg();
        let concat17_out1 = burn::tensor::Tensor::cat(
            [neg8_out1, squeeze15_out1].into(),
            3,
        );
        let mul37_out1 = concat17_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add18_out1 = mul36_out1.add(mul37_out1);
        let concat18_out1 = burn::tensor::Tensor::cat(
            [add18_out1, slice30_out1].into(),
            3,
        );
        let reshape51_out1 = concat18_out1.reshape([-1, 257, 64]);
        let transpose35_out1 = reshape51_out1.permute([0, 2, 1]);
        let reshape52_out1 = transpose35_out1.reshape([1, 24, 64, 257]);
        let mul38_out1 = concat16_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul39_out1 = reshape52_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul39_out1 = mul38_out1.matmul(mul39_out1);
        let softmax7_out1 = burn::tensor::activation::softmax(matmul39_out1, 3);
        let matmul40_out1 = softmax7_out1.matmul(transpose34_out1);
        let transpose36_out1 = matmul40_out1.permute([0, 2, 1, 3]);
        let reshape53_out1 = transpose36_out1.reshape([1, 257, 1536]);
        let linear29_out1 = self.linear29.forward(reshape53_out1);
        let add19_out1 = add16_out1.add(linear29_out1);
        let layernormalization11_out1 = {
            let dtype = add19_out1.clone().dtype();
            self.layernormalization11
                .forward(add19_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear30_out1 = self.linear30.forward(layernormalization11_out1);
        let reshape54_out1 = linear30_out1.reshape([1, 257, 24, 64]);
        let transpose37_out1 = reshape54_out1.permute([0, 2, 1, 3]);
        let linear31_out1 = self.linear31.forward(linear2_out1.clone());
        let split_tensors = linear31_out1.split(768, 2);
        let [split11_out1, split11_out2] = split_tensors.try_into().unwrap();
        let reshape55_out1 = split11_out1.reshape([1, 130, 12, 64]);
        let transpose38_out1 = reshape55_out1.permute([0, 2, 1, 3]);
        let reshape56_out1 = split11_out2.reshape([1, 130, 12, 64]);
        let transpose39_out1 = reshape56_out1.permute([0, 2, 1, 3]);
        let unsqueeze9_out1: Tensor<B, 5> = transpose38_out1.unsqueeze_dims::<5>(&[2]);
        let expand7_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze9_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze9_out1.expand(shape)
        };
        let unsqueeze10_out1: Tensor<B, 5> = transpose39_out1.unsqueeze_dims::<5>(&[2]);
        let expand8_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze10_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze10_out1.expand(shape)
        };
        let reshape57_out1 = expand8_out1.reshape([1, -1, 130, 64]);
        let reshape58_out1 = expand7_out1.reshape([24, 130, 64]);
        let transpose40_out1 = reshape58_out1.permute([0, 2, 1]);
        let reshape59_out1 = transpose40_out1.reshape([1, 24, 64, 130]);
        let mul40_out1 = transpose37_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul41_out1 = reshape59_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul44_out1 = mul40_out1.matmul(mul41_out1);
        let softmax8_out1 = burn::tensor::activation::softmax(matmul44_out1, 3);
        let matmul45_out1 = softmax8_out1.matmul(reshape57_out1);
        let transpose41_out1 = matmul45_out1.permute([0, 2, 1, 3]);
        let reshape60_out1 = transpose41_out1.reshape([1, 257, 1536]);
        let linear32_out1 = self.linear32.forward(reshape60_out1);
        let add20_out1 = add19_out1.add(linear32_out1);
        let layernormalization12_out1 = {
            let dtype = add20_out1.clone().dtype();
            self.layernormalization12
                .forward(add20_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear33_out1 = self.linear33.forward(layernormalization12_out1);
        let split_tensors = linear33_out1.split(6144, 2);
        let [split12_out1, split12_out2] = split_tensors.try_into().unwrap();
        let sigmoid6_out1 = burn::tensor::activation::sigmoid(split12_out2.clone());
        let mul42_out1 = split12_out2.mul(sigmoid6_out1);
        let mul43_out1 = split12_out1.mul(mul42_out1);
        let linear34_out1 = self.linear34.forward(mul43_out1);
        let add21_out1 = add20_out1.add(linear34_out1);
        let layernormalization13_out1 = {
            let dtype = add21_out1.clone().dtype();
            self.layernormalization13
                .forward(add21_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear35_out1 = self.linear35.forward(layernormalization13_out1);
        let split_tensors = linear35_out1.split(1536, 2);
        let [split13_out1, split13_out2, split13_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape61_out1 = split13_out1.reshape([1, 257, 24, 64]);
        let transpose42_out1 = reshape61_out1.permute([0, 2, 1, 3]);
        let reshape62_out1 = split13_out2.reshape([1, 257, 24, 64]);
        let transpose43_out1 = reshape62_out1.permute([0, 2, 1, 3]);
        let reshape63_out1 = split13_out3.reshape([1, 257, 24, 64]);
        let transpose44_out1 = reshape63_out1.permute([0, 2, 1, 3]);
        let slice33_out1 = transpose42_out1.clone().slice(s![.., .., .., 0..32]);
        let slice34_out1 = transpose42_out1.slice(s![.., .., .., 32..]);
        let mul44_out1 = slice33_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape64_out1 = slice33_out1.reshape([1, 24, 257, 2, 16]);
        let slice35_out1 = reshape64_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze17_out1 = slice35_out1.squeeze_dims::<4>(&[-2]);
        let slice36_out1 = reshape64_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze18_out1 = slice36_out1.squeeze_dims::<4>(&[-2]);
        let neg9_out1 = squeeze18_out1.neg();
        let concat19_out1 = burn::tensor::Tensor::cat(
            [neg9_out1, squeeze17_out1].into(),
            3,
        );
        let mul45_out1 = concat19_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add22_out1 = mul44_out1.add(mul45_out1);
        let concat20_out1 = burn::tensor::Tensor::cat(
            [add22_out1, slice34_out1].into(),
            3,
        );
        let slice37_out1 = transpose43_out1.clone().slice(s![.., .., .., 0..32]);
        let slice38_out1 = transpose43_out1.slice(s![.., .., .., 32..]);
        let mul46_out1 = slice37_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape65_out1 = slice37_out1.reshape([1, 24, 257, 2, 16]);
        let slice39_out1 = reshape65_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze19_out1 = slice39_out1.squeeze_dims::<4>(&[-2]);
        let slice40_out1 = reshape65_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze20_out1 = slice40_out1.squeeze_dims::<4>(&[-2]);
        let neg10_out1 = squeeze20_out1.neg();
        let concat21_out1 = burn::tensor::Tensor::cat(
            [neg10_out1, squeeze19_out1].into(),
            3,
        );
        let mul47_out1 = concat21_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add23_out1 = mul46_out1.add(mul47_out1);
        let concat22_out1 = burn::tensor::Tensor::cat(
            [add23_out1, slice38_out1].into(),
            3,
        );
        let reshape66_out1 = concat22_out1.reshape([-1, 257, 64]);
        let transpose45_out1 = reshape66_out1.permute([0, 2, 1]);
        let reshape67_out1 = transpose45_out1.reshape([1, 24, 64, 257]);
        let mul48_out1 = concat20_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul49_out1 = reshape67_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul50_out1 = mul48_out1.matmul(mul49_out1);
        let softmax9_out1 = burn::tensor::activation::softmax(matmul50_out1, 3);
        let matmul51_out1 = softmax9_out1.matmul(transpose44_out1);
        let transpose46_out1 = matmul51_out1.permute([0, 2, 1, 3]);
        let reshape68_out1 = transpose46_out1.reshape([1, 257, 1536]);
        let linear36_out1 = self.linear36.forward(reshape68_out1);
        let add24_out1 = add21_out1.add(linear36_out1);
        let layernormalization14_out1 = {
            let dtype = add24_out1.clone().dtype();
            self.layernormalization14
                .forward(add24_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear37_out1 = self.linear37.forward(layernormalization14_out1);
        let reshape69_out1 = linear37_out1.reshape([1, 257, 24, 64]);
        let transpose47_out1 = reshape69_out1.permute([0, 2, 1, 3]);
        let linear38_out1 = self.linear38.forward(linear2_out1.clone());
        let split_tensors = linear38_out1.split(768, 2);
        let [split14_out1, split14_out2] = split_tensors.try_into().unwrap();
        let reshape70_out1 = split14_out1.reshape([1, 130, 12, 64]);
        let transpose48_out1 = reshape70_out1.permute([0, 2, 1, 3]);
        let reshape71_out1 = split14_out2.reshape([1, 130, 12, 64]);
        let transpose49_out1 = reshape71_out1.permute([0, 2, 1, 3]);
        let unsqueeze11_out1: Tensor<B, 5> = transpose48_out1.unsqueeze_dims::<5>(&[2]);
        let expand9_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze11_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze11_out1.expand(shape)
        };
        let unsqueeze12_out1: Tensor<B, 5> = transpose49_out1.unsqueeze_dims::<5>(&[2]);
        let expand10_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze12_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze12_out1.expand(shape)
        };
        let reshape72_out1 = expand10_out1.reshape([1, -1, 130, 64]);
        let reshape73_out1 = expand9_out1.reshape([24, 130, 64]);
        let transpose50_out1 = reshape73_out1.permute([0, 2, 1]);
        let reshape74_out1 = transpose50_out1.reshape([1, 24, 64, 130]);
        let mul50_out1 = transpose47_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul51_out1 = reshape74_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul55_out1 = mul50_out1.matmul(mul51_out1);
        let softmax10_out1 = burn::tensor::activation::softmax(matmul55_out1, 3);
        let matmul56_out1 = softmax10_out1.matmul(reshape72_out1);
        let transpose51_out1 = matmul56_out1.permute([0, 2, 1, 3]);
        let reshape75_out1 = transpose51_out1.reshape([1, 257, 1536]);
        let linear39_out1 = self.linear39.forward(reshape75_out1);
        let add25_out1 = add24_out1.add(linear39_out1);
        let layernormalization15_out1 = {
            let dtype = add25_out1.clone().dtype();
            self.layernormalization15
                .forward(add25_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear40_out1 = self.linear40.forward(layernormalization15_out1);
        let split_tensors = linear40_out1.split(6144, 2);
        let [split15_out1, split15_out2] = split_tensors.try_into().unwrap();
        let sigmoid7_out1 = burn::tensor::activation::sigmoid(split15_out2.clone());
        let mul52_out1 = split15_out2.mul(sigmoid7_out1);
        let mul53_out1 = split15_out1.mul(mul52_out1);
        let linear41_out1 = self.linear41.forward(mul53_out1);
        let add26_out1 = add25_out1.add(linear41_out1);
        let layernormalization16_out1 = {
            let dtype = add26_out1.clone().dtype();
            self.layernormalization16
                .forward(add26_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear42_out1 = self.linear42.forward(layernormalization16_out1);
        let split_tensors = linear42_out1.split(1536, 2);
        let [split16_out1, split16_out2, split16_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape76_out1 = split16_out1.reshape([1, 257, 24, 64]);
        let transpose52_out1 = reshape76_out1.permute([0, 2, 1, 3]);
        let reshape77_out1 = split16_out2.reshape([1, 257, 24, 64]);
        let transpose53_out1 = reshape77_out1.permute([0, 2, 1, 3]);
        let reshape78_out1 = split16_out3.reshape([1, 257, 24, 64]);
        let transpose54_out1 = reshape78_out1.permute([0, 2, 1, 3]);
        let slice41_out1 = transpose52_out1.clone().slice(s![.., .., .., 0..32]);
        let slice42_out1 = transpose52_out1.slice(s![.., .., .., 32..]);
        let mul54_out1 = slice41_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape79_out1 = slice41_out1.reshape([1, 24, 257, 2, 16]);
        let slice43_out1 = reshape79_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze21_out1 = slice43_out1.squeeze_dims::<4>(&[-2]);
        let slice44_out1 = reshape79_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze22_out1 = slice44_out1.squeeze_dims::<4>(&[-2]);
        let neg11_out1 = squeeze22_out1.neg();
        let concat23_out1 = burn::tensor::Tensor::cat(
            [neg11_out1, squeeze21_out1].into(),
            3,
        );
        let mul55_out1 = concat23_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add27_out1 = mul54_out1.add(mul55_out1);
        let concat24_out1 = burn::tensor::Tensor::cat(
            [add27_out1, slice42_out1].into(),
            3,
        );
        let slice45_out1 = transpose53_out1.clone().slice(s![.., .., .., 0..32]);
        let slice46_out1 = transpose53_out1.slice(s![.., .., .., 32..]);
        let mul56_out1 = slice45_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape80_out1 = slice45_out1.reshape([1, 24, 257, 2, 16]);
        let slice47_out1 = reshape80_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze23_out1 = slice47_out1.squeeze_dims::<4>(&[-2]);
        let slice48_out1 = reshape80_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze24_out1 = slice48_out1.squeeze_dims::<4>(&[-2]);
        let neg12_out1 = squeeze24_out1.neg();
        let concat25_out1 = burn::tensor::Tensor::cat(
            [neg12_out1, squeeze23_out1].into(),
            3,
        );
        let mul57_out1 = concat25_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add28_out1 = mul56_out1.add(mul57_out1);
        let concat26_out1 = burn::tensor::Tensor::cat(
            [add28_out1, slice46_out1].into(),
            3,
        );
        let reshape81_out1 = concat26_out1.reshape([-1, 257, 64]);
        let transpose55_out1 = reshape81_out1.permute([0, 2, 1]);
        let reshape82_out1 = transpose55_out1.reshape([1, 24, 64, 257]);
        let mul58_out1 = concat24_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul59_out1 = reshape82_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul61_out1 = mul58_out1.matmul(mul59_out1);
        let softmax11_out1 = burn::tensor::activation::softmax(matmul61_out1, 3);
        let matmul62_out1 = softmax11_out1.matmul(transpose54_out1);
        let transpose56_out1 = matmul62_out1.permute([0, 2, 1, 3]);
        let reshape83_out1 = transpose56_out1.reshape([1, 257, 1536]);
        let linear43_out1 = self.linear43.forward(reshape83_out1);
        let add29_out1 = add26_out1.add(linear43_out1);
        let layernormalization17_out1 = {
            let dtype = add29_out1.clone().dtype();
            self.layernormalization17
                .forward(add29_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear44_out1 = self.linear44.forward(layernormalization17_out1);
        let reshape84_out1 = linear44_out1.reshape([1, 257, 24, 64]);
        let transpose57_out1 = reshape84_out1.permute([0, 2, 1, 3]);
        let linear45_out1 = self.linear45.forward(linear2_out1.clone());
        let split_tensors = linear45_out1.split(768, 2);
        let [split17_out1, split17_out2] = split_tensors.try_into().unwrap();
        let reshape85_out1 = split17_out1.reshape([1, 130, 12, 64]);
        let transpose58_out1 = reshape85_out1.permute([0, 2, 1, 3]);
        let reshape86_out1 = split17_out2.reshape([1, 130, 12, 64]);
        let transpose59_out1 = reshape86_out1.permute([0, 2, 1, 3]);
        let unsqueeze13_out1: Tensor<B, 5> = transpose58_out1.unsqueeze_dims::<5>(&[2]);
        let expand11_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze13_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze13_out1.expand(shape)
        };
        let unsqueeze14_out1: Tensor<B, 5> = transpose59_out1.unsqueeze_dims::<5>(&[2]);
        let expand12_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze14_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze14_out1.expand(shape)
        };
        let reshape87_out1 = expand12_out1.reshape([1, -1, 130, 64]);
        let reshape88_out1 = expand11_out1.reshape([24, 130, 64]);
        let transpose60_out1 = reshape88_out1.permute([0, 2, 1]);
        let reshape89_out1 = transpose60_out1.reshape([1, 24, 64, 130]);
        let mul60_out1 = transpose57_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul61_out1 = reshape89_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul66_out1 = mul60_out1.matmul(mul61_out1);
        let softmax12_out1 = burn::tensor::activation::softmax(matmul66_out1, 3);
        let matmul67_out1 = softmax12_out1.matmul(reshape87_out1);
        let transpose61_out1 = matmul67_out1.permute([0, 2, 1, 3]);
        let reshape90_out1 = transpose61_out1.reshape([1, 257, 1536]);
        let linear46_out1 = self.linear46.forward(reshape90_out1);
        let add30_out1 = add29_out1.add(linear46_out1);
        let layernormalization18_out1 = {
            let dtype = add30_out1.clone().dtype();
            self.layernormalization18
                .forward(add30_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear47_out1 = self.linear47.forward(layernormalization18_out1);
        let split_tensors = linear47_out1.split(6144, 2);
        let [split18_out1, split18_out2] = split_tensors.try_into().unwrap();
        let sigmoid8_out1 = burn::tensor::activation::sigmoid(split18_out2.clone());
        let mul62_out1 = split18_out2.mul(sigmoid8_out1);
        let mul63_out1 = split18_out1.mul(mul62_out1);
        let linear48_out1 = self.linear48.forward(mul63_out1);
        let add31_out1 = add30_out1.add(linear48_out1);
        let layernormalization19_out1 = {
            let dtype = add31_out1.clone().dtype();
            self.layernormalization19
                .forward(add31_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear49_out1 = self.linear49.forward(layernormalization19_out1);
        let split_tensors = linear49_out1.split(1536, 2);
        let [split19_out1, split19_out2, split19_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape91_out1 = split19_out1.reshape([1, 257, 24, 64]);
        let transpose62_out1 = reshape91_out1.permute([0, 2, 1, 3]);
        let reshape92_out1 = split19_out2.reshape([1, 257, 24, 64]);
        let transpose63_out1 = reshape92_out1.permute([0, 2, 1, 3]);
        let reshape93_out1 = split19_out3.reshape([1, 257, 24, 64]);
        let transpose64_out1 = reshape93_out1.permute([0, 2, 1, 3]);
        let slice49_out1 = transpose62_out1.clone().slice(s![.., .., .., 0..32]);
        let slice50_out1 = transpose62_out1.slice(s![.., .., .., 32..]);
        let mul64_out1 = slice49_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape94_out1 = slice49_out1.reshape([1, 24, 257, 2, 16]);
        let slice51_out1 = reshape94_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze25_out1 = slice51_out1.squeeze_dims::<4>(&[-2]);
        let slice52_out1 = reshape94_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze26_out1 = slice52_out1.squeeze_dims::<4>(&[-2]);
        let neg13_out1 = squeeze26_out1.neg();
        let concat27_out1 = burn::tensor::Tensor::cat(
            [neg13_out1, squeeze25_out1].into(),
            3,
        );
        let mul65_out1 = concat27_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add32_out1 = mul64_out1.add(mul65_out1);
        let concat28_out1 = burn::tensor::Tensor::cat(
            [add32_out1, slice50_out1].into(),
            3,
        );
        let slice53_out1 = transpose63_out1.clone().slice(s![.., .., .., 0..32]);
        let slice54_out1 = transpose63_out1.slice(s![.., .., .., 32..]);
        let mul66_out1 = slice53_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape95_out1 = slice53_out1.reshape([1, 24, 257, 2, 16]);
        let slice55_out1 = reshape95_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze27_out1 = slice55_out1.squeeze_dims::<4>(&[-2]);
        let slice56_out1 = reshape95_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze28_out1 = slice56_out1.squeeze_dims::<4>(&[-2]);
        let neg14_out1 = squeeze28_out1.neg();
        let concat29_out1 = burn::tensor::Tensor::cat(
            [neg14_out1, squeeze27_out1].into(),
            3,
        );
        let mul67_out1 = concat29_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add33_out1 = mul66_out1.add(mul67_out1);
        let concat30_out1 = burn::tensor::Tensor::cat(
            [add33_out1, slice54_out1].into(),
            3,
        );
        let reshape96_out1 = concat30_out1.reshape([-1, 257, 64]);
        let transpose65_out1 = reshape96_out1.permute([0, 2, 1]);
        let reshape97_out1 = transpose65_out1.reshape([1, 24, 64, 257]);
        let mul68_out1 = concat28_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul69_out1 = reshape97_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul72_out1 = mul68_out1.matmul(mul69_out1);
        let softmax13_out1 = burn::tensor::activation::softmax(matmul72_out1, 3);
        let matmul73_out1 = softmax13_out1.matmul(transpose64_out1);
        let transpose66_out1 = matmul73_out1.permute([0, 2, 1, 3]);
        let reshape98_out1 = transpose66_out1.reshape([1, 257, 1536]);
        let linear50_out1 = self.linear50.forward(reshape98_out1);
        let add34_out1 = add31_out1.add(linear50_out1);
        let layernormalization20_out1 = {
            let dtype = add34_out1.clone().dtype();
            self.layernormalization20
                .forward(add34_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear51_out1 = self.linear51.forward(layernormalization20_out1);
        let reshape99_out1 = linear51_out1.reshape([1, 257, 24, 64]);
        let transpose67_out1 = reshape99_out1.permute([0, 2, 1, 3]);
        let linear52_out1 = self.linear52.forward(linear2_out1.clone());
        let split_tensors = linear52_out1.split(768, 2);
        let [split20_out1, split20_out2] = split_tensors.try_into().unwrap();
        let reshape100_out1 = split20_out1.reshape([1, 130, 12, 64]);
        let transpose68_out1 = reshape100_out1.permute([0, 2, 1, 3]);
        let reshape101_out1 = split20_out2.reshape([1, 130, 12, 64]);
        let transpose69_out1 = reshape101_out1.permute([0, 2, 1, 3]);
        let unsqueeze15_out1: Tensor<B, 5> = transpose68_out1.unsqueeze_dims::<5>(&[2]);
        let expand13_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze15_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze15_out1.expand(shape)
        };
        let unsqueeze16_out1: Tensor<B, 5> = transpose69_out1.unsqueeze_dims::<5>(&[2]);
        let expand14_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze16_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze16_out1.expand(shape)
        };
        let reshape102_out1 = expand14_out1.reshape([1, -1, 130, 64]);
        let reshape103_out1 = expand13_out1.reshape([24, 130, 64]);
        let transpose70_out1 = reshape103_out1.permute([0, 2, 1]);
        let reshape104_out1 = transpose70_out1.reshape([1, 24, 64, 130]);
        let mul70_out1 = transpose67_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul71_out1 = reshape104_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul77_out1 = mul70_out1.matmul(mul71_out1);
        let softmax14_out1 = burn::tensor::activation::softmax(matmul77_out1, 3);
        let matmul78_out1 = softmax14_out1.matmul(reshape102_out1);
        let transpose71_out1 = matmul78_out1.permute([0, 2, 1, 3]);
        let reshape105_out1 = transpose71_out1.reshape([1, 257, 1536]);
        let linear53_out1 = self.linear53.forward(reshape105_out1);
        let add35_out1 = add34_out1.add(linear53_out1);
        let layernormalization21_out1 = {
            let dtype = add35_out1.clone().dtype();
            self.layernormalization21
                .forward(add35_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear54_out1 = self.linear54.forward(layernormalization21_out1);
        let split_tensors = linear54_out1.split(6144, 2);
        let [split21_out1, split21_out2] = split_tensors.try_into().unwrap();
        let sigmoid9_out1 = burn::tensor::activation::sigmoid(split21_out2.clone());
        let mul72_out1 = split21_out2.mul(sigmoid9_out1);
        let mul73_out1 = split21_out1.mul(mul72_out1);
        let linear55_out1 = self.linear55.forward(mul73_out1);
        let add36_out1 = add35_out1.add(linear55_out1);
        let layernormalization22_out1 = {
            let dtype = add36_out1.clone().dtype();
            self.layernormalization22
                .forward(add36_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear56_out1 = self.linear56.forward(layernormalization22_out1);
        let split_tensors = linear56_out1.split(1536, 2);
        let [split22_out1, split22_out2, split22_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape106_out1 = split22_out1.reshape([1, 257, 24, 64]);
        let transpose72_out1 = reshape106_out1.permute([0, 2, 1, 3]);
        let reshape107_out1 = split22_out2.reshape([1, 257, 24, 64]);
        let transpose73_out1 = reshape107_out1.permute([0, 2, 1, 3]);
        let reshape108_out1 = split22_out3.reshape([1, 257, 24, 64]);
        let transpose74_out1 = reshape108_out1.permute([0, 2, 1, 3]);
        let slice57_out1 = transpose72_out1.clone().slice(s![.., .., .., 0..32]);
        let slice58_out1 = transpose72_out1.slice(s![.., .., .., 32..]);
        let mul74_out1 = slice57_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape109_out1 = slice57_out1.reshape([1, 24, 257, 2, 16]);
        let slice59_out1 = reshape109_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze29_out1 = slice59_out1.squeeze_dims::<4>(&[-2]);
        let slice60_out1 = reshape109_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze30_out1 = slice60_out1.squeeze_dims::<4>(&[-2]);
        let neg15_out1 = squeeze30_out1.neg();
        let concat31_out1 = burn::tensor::Tensor::cat(
            [neg15_out1, squeeze29_out1].into(),
            3,
        );
        let mul75_out1 = concat31_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add37_out1 = mul74_out1.add(mul75_out1);
        let concat32_out1 = burn::tensor::Tensor::cat(
            [add37_out1, slice58_out1].into(),
            3,
        );
        let slice61_out1 = transpose73_out1.clone().slice(s![.., .., .., 0..32]);
        let slice62_out1 = transpose73_out1.slice(s![.., .., .., 32..]);
        let mul76_out1 = slice61_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape110_out1 = slice61_out1.reshape([1, 24, 257, 2, 16]);
        let slice63_out1 = reshape110_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze31_out1 = slice63_out1.squeeze_dims::<4>(&[-2]);
        let slice64_out1 = reshape110_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze32_out1 = slice64_out1.squeeze_dims::<4>(&[-2]);
        let neg16_out1 = squeeze32_out1.neg();
        let concat33_out1 = burn::tensor::Tensor::cat(
            [neg16_out1, squeeze31_out1].into(),
            3,
        );
        let mul77_out1 = concat33_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add38_out1 = mul76_out1.add(mul77_out1);
        let concat34_out1 = burn::tensor::Tensor::cat(
            [add38_out1, slice62_out1].into(),
            3,
        );
        let reshape111_out1 = concat34_out1.reshape([-1, 257, 64]);
        let transpose75_out1 = reshape111_out1.permute([0, 2, 1]);
        let reshape112_out1 = transpose75_out1.reshape([1, 24, 64, 257]);
        let mul78_out1 = concat32_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul79_out1 = reshape112_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul83_out1 = mul78_out1.matmul(mul79_out1);
        let softmax15_out1 = burn::tensor::activation::softmax(matmul83_out1, 3);
        let matmul84_out1 = softmax15_out1.matmul(transpose74_out1);
        let transpose76_out1 = matmul84_out1.permute([0, 2, 1, 3]);
        let reshape113_out1 = transpose76_out1.reshape([1, 257, 1536]);
        let linear57_out1 = self.linear57.forward(reshape113_out1);
        let add39_out1 = add36_out1.add(linear57_out1);
        let layernormalization23_out1 = {
            let dtype = add39_out1.clone().dtype();
            self.layernormalization23
                .forward(add39_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear58_out1 = self.linear58.forward(layernormalization23_out1);
        let reshape114_out1 = linear58_out1.reshape([1, 257, 24, 64]);
        let transpose77_out1 = reshape114_out1.permute([0, 2, 1, 3]);
        let linear59_out1 = self.linear59.forward(linear2_out1.clone());
        let split_tensors = linear59_out1.split(768, 2);
        let [split23_out1, split23_out2] = split_tensors.try_into().unwrap();
        let reshape115_out1 = split23_out1.reshape([1, 130, 12, 64]);
        let transpose78_out1 = reshape115_out1.permute([0, 2, 1, 3]);
        let reshape116_out1 = split23_out2.reshape([1, 130, 12, 64]);
        let transpose79_out1 = reshape116_out1.permute([0, 2, 1, 3]);
        let unsqueeze17_out1: Tensor<B, 5> = transpose78_out1.unsqueeze_dims::<5>(&[2]);
        let expand15_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze17_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze17_out1.expand(shape)
        };
        let unsqueeze18_out1: Tensor<B, 5> = transpose79_out1.unsqueeze_dims::<5>(&[2]);
        let expand16_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze18_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze18_out1.expand(shape)
        };
        let reshape117_out1 = expand16_out1.reshape([1, -1, 130, 64]);
        let reshape118_out1 = expand15_out1.reshape([24, 130, 64]);
        let transpose80_out1 = reshape118_out1.permute([0, 2, 1]);
        let reshape119_out1 = transpose80_out1.reshape([1, 24, 64, 130]);
        let mul80_out1 = transpose77_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul81_out1 = reshape119_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul88_out1 = mul80_out1.matmul(mul81_out1);
        let softmax16_out1 = burn::tensor::activation::softmax(matmul88_out1, 3);
        let matmul89_out1 = softmax16_out1.matmul(reshape117_out1);
        let transpose81_out1 = matmul89_out1.permute([0, 2, 1, 3]);
        let reshape120_out1 = transpose81_out1.reshape([1, 257, 1536]);
        let linear60_out1 = self.linear60.forward(reshape120_out1);
        let add40_out1 = add39_out1.add(linear60_out1);
        let layernormalization24_out1 = {
            let dtype = add40_out1.clone().dtype();
            self.layernormalization24
                .forward(add40_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear61_out1 = self.linear61.forward(layernormalization24_out1);
        let split_tensors = linear61_out1.split(6144, 2);
        let [split24_out1, split24_out2] = split_tensors.try_into().unwrap();
        let sigmoid10_out1 = burn::tensor::activation::sigmoid(split24_out2.clone());
        let mul82_out1 = split24_out2.mul(sigmoid10_out1);
        let mul83_out1 = split24_out1.mul(mul82_out1);
        let linear62_out1 = self.linear62.forward(mul83_out1);
        let add41_out1 = add40_out1.add(linear62_out1);
        let layernormalization25_out1 = {
            let dtype = add41_out1.clone().dtype();
            self.layernormalization25
                .forward(add41_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear63_out1 = self.linear63.forward(layernormalization25_out1);
        let split_tensors = linear63_out1.split(1536, 2);
        let [split25_out1, split25_out2, split25_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape121_out1 = split25_out1.reshape([1, 257, 24, 64]);
        let transpose82_out1 = reshape121_out1.permute([0, 2, 1, 3]);
        let reshape122_out1 = split25_out2.reshape([1, 257, 24, 64]);
        let transpose83_out1 = reshape122_out1.permute([0, 2, 1, 3]);
        let reshape123_out1 = split25_out3.reshape([1, 257, 24, 64]);
        let transpose84_out1 = reshape123_out1.permute([0, 2, 1, 3]);
        let slice65_out1 = transpose82_out1.clone().slice(s![.., .., .., 0..32]);
        let slice66_out1 = transpose82_out1.slice(s![.., .., .., 32..]);
        let mul84_out1 = slice65_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape124_out1 = slice65_out1.reshape([1, 24, 257, 2, 16]);
        let slice67_out1 = reshape124_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze33_out1 = slice67_out1.squeeze_dims::<4>(&[-2]);
        let slice68_out1 = reshape124_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze34_out1 = slice68_out1.squeeze_dims::<4>(&[-2]);
        let neg17_out1 = squeeze34_out1.neg();
        let concat35_out1 = burn::tensor::Tensor::cat(
            [neg17_out1, squeeze33_out1].into(),
            3,
        );
        let mul85_out1 = concat35_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add42_out1 = mul84_out1.add(mul85_out1);
        let concat36_out1 = burn::tensor::Tensor::cat(
            [add42_out1, slice66_out1].into(),
            3,
        );
        let slice69_out1 = transpose83_out1.clone().slice(s![.., .., .., 0..32]);
        let slice70_out1 = transpose83_out1.slice(s![.., .., .., 32..]);
        let mul86_out1 = slice69_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape125_out1 = slice69_out1.reshape([1, 24, 257, 2, 16]);
        let slice71_out1 = reshape125_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze35_out1 = slice71_out1.squeeze_dims::<4>(&[-2]);
        let slice72_out1 = reshape125_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze36_out1 = slice72_out1.squeeze_dims::<4>(&[-2]);
        let neg18_out1 = squeeze36_out1.neg();
        let concat37_out1 = burn::tensor::Tensor::cat(
            [neg18_out1, squeeze35_out1].into(),
            3,
        );
        let mul87_out1 = concat37_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add43_out1 = mul86_out1.add(mul87_out1);
        let concat38_out1 = burn::tensor::Tensor::cat(
            [add43_out1, slice70_out1].into(),
            3,
        );
        let reshape126_out1 = concat38_out1.reshape([-1, 257, 64]);
        let transpose85_out1 = reshape126_out1.permute([0, 2, 1]);
        let reshape127_out1 = transpose85_out1.reshape([1, 24, 64, 257]);
        let mul88_out1 = concat36_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul89_out1 = reshape127_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul94_out1 = mul88_out1.matmul(mul89_out1);
        let softmax17_out1 = burn::tensor::activation::softmax(matmul94_out1, 3);
        let matmul95_out1 = softmax17_out1.matmul(transpose84_out1);
        let transpose86_out1 = matmul95_out1.permute([0, 2, 1, 3]);
        let reshape128_out1 = transpose86_out1.reshape([1, 257, 1536]);
        let linear64_out1 = self.linear64.forward(reshape128_out1);
        let add44_out1 = add41_out1.add(linear64_out1);
        let layernormalization26_out1 = {
            let dtype = add44_out1.clone().dtype();
            self.layernormalization26
                .forward(add44_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear65_out1 = self.linear65.forward(layernormalization26_out1);
        let reshape129_out1 = linear65_out1.reshape([1, 257, 24, 64]);
        let transpose87_out1 = reshape129_out1.permute([0, 2, 1, 3]);
        let linear66_out1 = self.linear66.forward(linear2_out1.clone());
        let split_tensors = linear66_out1.split(768, 2);
        let [split26_out1, split26_out2] = split_tensors.try_into().unwrap();
        let reshape130_out1 = split26_out1.reshape([1, 130, 12, 64]);
        let transpose88_out1 = reshape130_out1.permute([0, 2, 1, 3]);
        let reshape131_out1 = split26_out2.reshape([1, 130, 12, 64]);
        let transpose89_out1 = reshape131_out1.permute([0, 2, 1, 3]);
        let unsqueeze19_out1: Tensor<B, 5> = transpose88_out1.unsqueeze_dims::<5>(&[2]);
        let expand17_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze19_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze19_out1.expand(shape)
        };
        let unsqueeze20_out1: Tensor<B, 5> = transpose89_out1.unsqueeze_dims::<5>(&[2]);
        let expand18_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze20_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze20_out1.expand(shape)
        };
        let reshape132_out1 = expand18_out1.reshape([1, -1, 130, 64]);
        let reshape133_out1 = expand17_out1.reshape([24, 130, 64]);
        let transpose90_out1 = reshape133_out1.permute([0, 2, 1]);
        let reshape134_out1 = transpose90_out1.reshape([1, 24, 64, 130]);
        let mul90_out1 = transpose87_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul91_out1 = reshape134_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul99_out1 = mul90_out1.matmul(mul91_out1);
        let softmax18_out1 = burn::tensor::activation::softmax(matmul99_out1, 3);
        let matmul100_out1 = softmax18_out1.matmul(reshape132_out1);
        let transpose91_out1 = matmul100_out1.permute([0, 2, 1, 3]);
        let reshape135_out1 = transpose91_out1.reshape([1, 257, 1536]);
        let linear67_out1 = self.linear67.forward(reshape135_out1);
        let add45_out1 = add44_out1.add(linear67_out1);
        let layernormalization27_out1 = {
            let dtype = add45_out1.clone().dtype();
            self.layernormalization27
                .forward(add45_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear68_out1 = self.linear68.forward(layernormalization27_out1);
        let split_tensors = linear68_out1.split(6144, 2);
        let [split27_out1, split27_out2] = split_tensors.try_into().unwrap();
        let sigmoid11_out1 = burn::tensor::activation::sigmoid(split27_out2.clone());
        let mul92_out1 = split27_out2.mul(sigmoid11_out1);
        let mul93_out1 = split27_out1.mul(mul92_out1);
        let linear69_out1 = self.linear69.forward(mul93_out1);
        let add46_out1 = add45_out1.add(linear69_out1);
        let layernormalization28_out1 = {
            let dtype = add46_out1.clone().dtype();
            self.layernormalization28
                .forward(add46_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear70_out1 = self.linear70.forward(layernormalization28_out1);
        let split_tensors = linear70_out1.split(1536, 2);
        let [split28_out1, split28_out2, split28_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape136_out1 = split28_out1.reshape([1, 257, 24, 64]);
        let transpose92_out1 = reshape136_out1.permute([0, 2, 1, 3]);
        let reshape137_out1 = split28_out2.reshape([1, 257, 24, 64]);
        let transpose93_out1 = reshape137_out1.permute([0, 2, 1, 3]);
        let reshape138_out1 = split28_out3.reshape([1, 257, 24, 64]);
        let transpose94_out1 = reshape138_out1.permute([0, 2, 1, 3]);
        let slice73_out1 = transpose92_out1.clone().slice(s![.., .., .., 0..32]);
        let slice74_out1 = transpose92_out1.slice(s![.., .., .., 32..]);
        let mul94_out1 = slice73_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape139_out1 = slice73_out1.reshape([1, 24, 257, 2, 16]);
        let slice75_out1 = reshape139_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze37_out1 = slice75_out1.squeeze_dims::<4>(&[-2]);
        let slice76_out1 = reshape139_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze38_out1 = slice76_out1.squeeze_dims::<4>(&[-2]);
        let neg19_out1 = squeeze38_out1.neg();
        let concat39_out1 = burn::tensor::Tensor::cat(
            [neg19_out1, squeeze37_out1].into(),
            3,
        );
        let mul95_out1 = concat39_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add47_out1 = mul94_out1.add(mul95_out1);
        let concat40_out1 = burn::tensor::Tensor::cat(
            [add47_out1, slice74_out1].into(),
            3,
        );
        let slice77_out1 = transpose93_out1.clone().slice(s![.., .., .., 0..32]);
        let slice78_out1 = transpose93_out1.slice(s![.., .., .., 32..]);
        let mul96_out1 = slice77_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape140_out1 = slice77_out1.reshape([1, 24, 257, 2, 16]);
        let slice79_out1 = reshape140_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze39_out1 = slice79_out1.squeeze_dims::<4>(&[-2]);
        let slice80_out1 = reshape140_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze40_out1 = slice80_out1.squeeze_dims::<4>(&[-2]);
        let neg20_out1 = squeeze40_out1.neg();
        let concat41_out1 = burn::tensor::Tensor::cat(
            [neg20_out1, squeeze39_out1].into(),
            3,
        );
        let mul97_out1 = concat41_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add48_out1 = mul96_out1.add(mul97_out1);
        let concat42_out1 = burn::tensor::Tensor::cat(
            [add48_out1, slice78_out1].into(),
            3,
        );
        let reshape141_out1 = concat42_out1.reshape([-1, 257, 64]);
        let transpose95_out1 = reshape141_out1.permute([0, 2, 1]);
        let reshape142_out1 = transpose95_out1.reshape([1, 24, 64, 257]);
        let mul98_out1 = concat40_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul99_out1 = reshape142_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul105_out1 = mul98_out1.matmul(mul99_out1);
        let softmax19_out1 = burn::tensor::activation::softmax(matmul105_out1, 3);
        let matmul106_out1 = softmax19_out1.matmul(transpose94_out1);
        let transpose96_out1 = matmul106_out1.permute([0, 2, 1, 3]);
        let reshape143_out1 = transpose96_out1.reshape([1, 257, 1536]);
        let linear71_out1 = self.linear71.forward(reshape143_out1);
        let add49_out1 = add46_out1.add(linear71_out1);
        let layernormalization29_out1 = {
            let dtype = add49_out1.clone().dtype();
            self.layernormalization29
                .forward(add49_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear72_out1 = self.linear72.forward(layernormalization29_out1);
        let reshape144_out1 = linear72_out1.reshape([1, 257, 24, 64]);
        let transpose97_out1 = reshape144_out1.permute([0, 2, 1, 3]);
        let linear73_out1 = self.linear73.forward(linear2_out1.clone());
        let split_tensors = linear73_out1.split(768, 2);
        let [split29_out1, split29_out2] = split_tensors.try_into().unwrap();
        let reshape145_out1 = split29_out1.reshape([1, 130, 12, 64]);
        let transpose98_out1 = reshape145_out1.permute([0, 2, 1, 3]);
        let reshape146_out1 = split29_out2.reshape([1, 130, 12, 64]);
        let transpose99_out1 = reshape146_out1.permute([0, 2, 1, 3]);
        let unsqueeze21_out1: Tensor<B, 5> = transpose98_out1.unsqueeze_dims::<5>(&[2]);
        let expand19_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze21_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze21_out1.expand(shape)
        };
        let unsqueeze22_out1: Tensor<B, 5> = transpose99_out1.unsqueeze_dims::<5>(&[2]);
        let expand20_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze22_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze22_out1.expand(shape)
        };
        let reshape147_out1 = expand20_out1.reshape([1, -1, 130, 64]);
        let reshape148_out1 = expand19_out1.reshape([24, 130, 64]);
        let transpose100_out1 = reshape148_out1.permute([0, 2, 1]);
        let reshape149_out1 = transpose100_out1.reshape([1, 24, 64, 130]);
        let mul100_out1 = transpose97_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul101_out1 = reshape149_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul110_out1 = mul100_out1.matmul(mul101_out1);
        let softmax20_out1 = burn::tensor::activation::softmax(matmul110_out1, 3);
        let matmul111_out1 = softmax20_out1.matmul(reshape147_out1);
        let transpose101_out1 = matmul111_out1.permute([0, 2, 1, 3]);
        let reshape150_out1 = transpose101_out1.reshape([1, 257, 1536]);
        let linear74_out1 = self.linear74.forward(reshape150_out1);
        let add50_out1 = add49_out1.add(linear74_out1);
        let layernormalization30_out1 = {
            let dtype = add50_out1.clone().dtype();
            self.layernormalization30
                .forward(add50_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear75_out1 = self.linear75.forward(layernormalization30_out1);
        let split_tensors = linear75_out1.split(6144, 2);
        let [split30_out1, split30_out2] = split_tensors.try_into().unwrap();
        let sigmoid12_out1 = burn::tensor::activation::sigmoid(split30_out2.clone());
        let mul102_out1 = split30_out2.mul(sigmoid12_out1);
        let mul103_out1 = split30_out1.mul(mul102_out1);
        let linear76_out1 = self.linear76.forward(mul103_out1);
        let add51_out1 = add50_out1.add(linear76_out1);
        let layernormalization31_out1 = {
            let dtype = add51_out1.clone().dtype();
            self.layernormalization31
                .forward(add51_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear77_out1 = self.linear77.forward(layernormalization31_out1);
        let split_tensors = linear77_out1.split(1536, 2);
        let [split31_out1, split31_out2, split31_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape151_out1 = split31_out1.reshape([1, 257, 24, 64]);
        let transpose102_out1 = reshape151_out1.permute([0, 2, 1, 3]);
        let reshape152_out1 = split31_out2.reshape([1, 257, 24, 64]);
        let transpose103_out1 = reshape152_out1.permute([0, 2, 1, 3]);
        let reshape153_out1 = split31_out3.reshape([1, 257, 24, 64]);
        let transpose104_out1 = reshape153_out1.permute([0, 2, 1, 3]);
        let slice81_out1 = transpose102_out1.clone().slice(s![.., .., .., 0..32]);
        let slice82_out1 = transpose102_out1.slice(s![.., .., .., 32..]);
        let mul104_out1 = slice81_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape154_out1 = slice81_out1.reshape([1, 24, 257, 2, 16]);
        let slice83_out1 = reshape154_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze41_out1 = slice83_out1.squeeze_dims::<4>(&[-2]);
        let slice84_out1 = reshape154_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze42_out1 = slice84_out1.squeeze_dims::<4>(&[-2]);
        let neg21_out1 = squeeze42_out1.neg();
        let concat43_out1 = burn::tensor::Tensor::cat(
            [neg21_out1, squeeze41_out1].into(),
            3,
        );
        let mul105_out1 = concat43_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add52_out1 = mul104_out1.add(mul105_out1);
        let concat44_out1 = burn::tensor::Tensor::cat(
            [add52_out1, slice82_out1].into(),
            3,
        );
        let slice85_out1 = transpose103_out1.clone().slice(s![.., .., .., 0..32]);
        let slice86_out1 = transpose103_out1.slice(s![.., .., .., 32..]);
        let mul106_out1 = slice85_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape155_out1 = slice85_out1.reshape([1, 24, 257, 2, 16]);
        let slice87_out1 = reshape155_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze43_out1 = slice87_out1.squeeze_dims::<4>(&[-2]);
        let slice88_out1 = reshape155_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze44_out1 = slice88_out1.squeeze_dims::<4>(&[-2]);
        let neg22_out1 = squeeze44_out1.neg();
        let concat45_out1 = burn::tensor::Tensor::cat(
            [neg22_out1, squeeze43_out1].into(),
            3,
        );
        let mul107_out1 = concat45_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add53_out1 = mul106_out1.add(mul107_out1);
        let concat46_out1 = burn::tensor::Tensor::cat(
            [add53_out1, slice86_out1].into(),
            3,
        );
        let reshape156_out1 = concat46_out1.reshape([-1, 257, 64]);
        let transpose105_out1 = reshape156_out1.permute([0, 2, 1]);
        let reshape157_out1 = transpose105_out1.reshape([1, 24, 64, 257]);
        let mul108_out1 = concat44_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul109_out1 = reshape157_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul116_out1 = mul108_out1.matmul(mul109_out1);
        let softmax21_out1 = burn::tensor::activation::softmax(matmul116_out1, 3);
        let matmul117_out1 = softmax21_out1.matmul(transpose104_out1);
        let transpose106_out1 = matmul117_out1.permute([0, 2, 1, 3]);
        let reshape158_out1 = transpose106_out1.reshape([1, 257, 1536]);
        let linear78_out1 = self.linear78.forward(reshape158_out1);
        let add54_out1 = add51_out1.add(linear78_out1);
        let layernormalization32_out1 = {
            let dtype = add54_out1.clone().dtype();
            self.layernormalization32
                .forward(add54_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear79_out1 = self.linear79.forward(layernormalization32_out1);
        let reshape159_out1 = linear79_out1.reshape([1, 257, 24, 64]);
        let transpose107_out1 = reshape159_out1.permute([0, 2, 1, 3]);
        let linear80_out1 = self.linear80.forward(linear2_out1.clone());
        let split_tensors = linear80_out1.split(768, 2);
        let [split32_out1, split32_out2] = split_tensors.try_into().unwrap();
        let reshape160_out1 = split32_out1.reshape([1, 130, 12, 64]);
        let transpose108_out1 = reshape160_out1.permute([0, 2, 1, 3]);
        let reshape161_out1 = split32_out2.reshape([1, 130, 12, 64]);
        let transpose109_out1 = reshape161_out1.permute([0, 2, 1, 3]);
        let unsqueeze23_out1: Tensor<B, 5> = transpose108_out1.unsqueeze_dims::<5>(&[2]);
        let expand21_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze23_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze23_out1.expand(shape)
        };
        let unsqueeze24_out1: Tensor<B, 5> = transpose109_out1.unsqueeze_dims::<5>(&[2]);
        let expand22_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze24_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze24_out1.expand(shape)
        };
        let reshape162_out1 = expand22_out1.reshape([1, -1, 130, 64]);
        let reshape163_out1 = expand21_out1.reshape([24, 130, 64]);
        let transpose110_out1 = reshape163_out1.permute([0, 2, 1]);
        let reshape164_out1 = transpose110_out1.reshape([1, 24, 64, 130]);
        let mul110_out1 = transpose107_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul111_out1 = reshape164_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul121_out1 = mul110_out1.matmul(mul111_out1);
        let softmax22_out1 = burn::tensor::activation::softmax(matmul121_out1, 3);
        let matmul122_out1 = softmax22_out1.matmul(reshape162_out1);
        let transpose111_out1 = matmul122_out1.permute([0, 2, 1, 3]);
        let reshape165_out1 = transpose111_out1.reshape([1, 257, 1536]);
        let linear81_out1 = self.linear81.forward(reshape165_out1);
        let add55_out1 = add54_out1.add(linear81_out1);
        let layernormalization33_out1 = {
            let dtype = add55_out1.clone().dtype();
            self.layernormalization33
                .forward(add55_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear82_out1 = self.linear82.forward(layernormalization33_out1);
        let split_tensors = linear82_out1.split(6144, 2);
        let [split33_out1, split33_out2] = split_tensors.try_into().unwrap();
        let sigmoid13_out1 = burn::tensor::activation::sigmoid(split33_out2.clone());
        let mul112_out1 = split33_out2.mul(sigmoid13_out1);
        let mul113_out1 = split33_out1.mul(mul112_out1);
        let linear83_out1 = self.linear83.forward(mul113_out1);
        let add56_out1 = add55_out1.add(linear83_out1);
        let layernormalization34_out1 = {
            let dtype = add56_out1.clone().dtype();
            self.layernormalization34
                .forward(add56_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear84_out1 = self.linear84.forward(layernormalization34_out1);
        let split_tensors = linear84_out1.split(1536, 2);
        let [split34_out1, split34_out2, split34_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape166_out1 = split34_out1.reshape([1, 257, 24, 64]);
        let transpose112_out1 = reshape166_out1.permute([0, 2, 1, 3]);
        let reshape167_out1 = split34_out2.reshape([1, 257, 24, 64]);
        let transpose113_out1 = reshape167_out1.permute([0, 2, 1, 3]);
        let reshape168_out1 = split34_out3.reshape([1, 257, 24, 64]);
        let transpose114_out1 = reshape168_out1.permute([0, 2, 1, 3]);
        let slice89_out1 = transpose112_out1.clone().slice(s![.., .., .., 0..32]);
        let slice90_out1 = transpose112_out1.slice(s![.., .., .., 32..]);
        let mul114_out1 = slice89_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape169_out1 = slice89_out1.reshape([1, 24, 257, 2, 16]);
        let slice91_out1 = reshape169_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze45_out1 = slice91_out1.squeeze_dims::<4>(&[-2]);
        let slice92_out1 = reshape169_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze46_out1 = slice92_out1.squeeze_dims::<4>(&[-2]);
        let neg23_out1 = squeeze46_out1.neg();
        let concat47_out1 = burn::tensor::Tensor::cat(
            [neg23_out1, squeeze45_out1].into(),
            3,
        );
        let mul115_out1 = concat47_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add57_out1 = mul114_out1.add(mul115_out1);
        let concat48_out1 = burn::tensor::Tensor::cat(
            [add57_out1, slice90_out1].into(),
            3,
        );
        let slice93_out1 = transpose113_out1.clone().slice(s![.., .., .., 0..32]);
        let slice94_out1 = transpose113_out1.slice(s![.., .., .., 32..]);
        let mul116_out1 = slice93_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape170_out1 = slice93_out1.reshape([1, 24, 257, 2, 16]);
        let slice95_out1 = reshape170_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze47_out1 = slice95_out1.squeeze_dims::<4>(&[-2]);
        let slice96_out1 = reshape170_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze48_out1 = slice96_out1.squeeze_dims::<4>(&[-2]);
        let neg24_out1 = squeeze48_out1.neg();
        let concat49_out1 = burn::tensor::Tensor::cat(
            [neg24_out1, squeeze47_out1].into(),
            3,
        );
        let mul117_out1 = concat49_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add58_out1 = mul116_out1.add(mul117_out1);
        let concat50_out1 = burn::tensor::Tensor::cat(
            [add58_out1, slice94_out1].into(),
            3,
        );
        let reshape171_out1 = concat50_out1.reshape([-1, 257, 64]);
        let transpose115_out1 = reshape171_out1.permute([0, 2, 1]);
        let reshape172_out1 = transpose115_out1.reshape([1, 24, 64, 257]);
        let mul118_out1 = concat48_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul119_out1 = reshape172_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul127_out1 = mul118_out1.matmul(mul119_out1);
        let softmax23_out1 = burn::tensor::activation::softmax(matmul127_out1, 3);
        let matmul128_out1 = softmax23_out1.matmul(transpose114_out1);
        let transpose116_out1 = matmul128_out1.permute([0, 2, 1, 3]);
        let reshape173_out1 = transpose116_out1.reshape([1, 257, 1536]);
        let linear85_out1 = self.linear85.forward(reshape173_out1);
        let add59_out1 = add56_out1.add(linear85_out1);
        let layernormalization35_out1 = {
            let dtype = add59_out1.clone().dtype();
            self.layernormalization35
                .forward(add59_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear86_out1 = self.linear86.forward(layernormalization35_out1);
        let reshape174_out1 = linear86_out1.reshape([1, 257, 24, 64]);
        let transpose117_out1 = reshape174_out1.permute([0, 2, 1, 3]);
        let linear87_out1 = self.linear87.forward(linear2_out1.clone());
        let split_tensors = linear87_out1.split(768, 2);
        let [split35_out1, split35_out2] = split_tensors.try_into().unwrap();
        let reshape175_out1 = split35_out1.reshape([1, 130, 12, 64]);
        let transpose118_out1 = reshape175_out1.permute([0, 2, 1, 3]);
        let reshape176_out1 = split35_out2.reshape([1, 130, 12, 64]);
        let transpose119_out1 = reshape176_out1.permute([0, 2, 1, 3]);
        let unsqueeze25_out1: Tensor<B, 5> = transpose118_out1.unsqueeze_dims::<5>(&[2]);
        let expand23_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze25_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze25_out1.expand(shape)
        };
        let unsqueeze26_out1: Tensor<B, 5> = transpose119_out1.unsqueeze_dims::<5>(&[2]);
        let expand24_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze26_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze26_out1.expand(shape)
        };
        let reshape177_out1 = expand24_out1.reshape([1, -1, 130, 64]);
        let reshape178_out1 = expand23_out1.reshape([24, 130, 64]);
        let transpose120_out1 = reshape178_out1.permute([0, 2, 1]);
        let reshape179_out1 = transpose120_out1.reshape([1, 24, 64, 130]);
        let mul120_out1 = transpose117_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul121_out1 = reshape179_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul132_out1 = mul120_out1.matmul(mul121_out1);
        let softmax24_out1 = burn::tensor::activation::softmax(matmul132_out1, 3);
        let matmul133_out1 = softmax24_out1.matmul(reshape177_out1);
        let transpose121_out1 = matmul133_out1.permute([0, 2, 1, 3]);
        let reshape180_out1 = transpose121_out1.reshape([1, 257, 1536]);
        let linear88_out1 = self.linear88.forward(reshape180_out1);
        let add60_out1 = add59_out1.add(linear88_out1);
        let layernormalization36_out1 = {
            let dtype = add60_out1.clone().dtype();
            self.layernormalization36
                .forward(add60_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear89_out1 = self.linear89.forward(layernormalization36_out1);
        let split_tensors = linear89_out1.split(6144, 2);
        let [split36_out1, split36_out2] = split_tensors.try_into().unwrap();
        let sigmoid14_out1 = burn::tensor::activation::sigmoid(split36_out2.clone());
        let mul122_out1 = split36_out2.mul(sigmoid14_out1);
        let mul123_out1 = split36_out1.mul(mul122_out1);
        let linear90_out1 = self.linear90.forward(mul123_out1);
        let add61_out1 = add60_out1.add(linear90_out1);
        let layernormalization37_out1 = {
            let dtype = add61_out1.clone().dtype();
            self.layernormalization37
                .forward(add61_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear91_out1 = self.linear91.forward(layernormalization37_out1);
        let split_tensors = linear91_out1.split(1536, 2);
        let [split37_out1, split37_out2, split37_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape181_out1 = split37_out1.reshape([1, 257, 24, 64]);
        let transpose122_out1 = reshape181_out1.permute([0, 2, 1, 3]);
        let reshape182_out1 = split37_out2.reshape([1, 257, 24, 64]);
        let transpose123_out1 = reshape182_out1.permute([0, 2, 1, 3]);
        let reshape183_out1 = split37_out3.reshape([1, 257, 24, 64]);
        let transpose124_out1 = reshape183_out1.permute([0, 2, 1, 3]);
        let slice97_out1 = transpose122_out1.clone().slice(s![.., .., .., 0..32]);
        let slice98_out1 = transpose122_out1.slice(s![.., .., .., 32..]);
        let mul124_out1 = slice97_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape184_out1 = slice97_out1.reshape([1, 24, 257, 2, 16]);
        let slice99_out1 = reshape184_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze49_out1 = slice99_out1.squeeze_dims::<4>(&[-2]);
        let slice100_out1 = reshape184_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze50_out1 = slice100_out1.squeeze_dims::<4>(&[-2]);
        let neg25_out1 = squeeze50_out1.neg();
        let concat51_out1 = burn::tensor::Tensor::cat(
            [neg25_out1, squeeze49_out1].into(),
            3,
        );
        let mul125_out1 = concat51_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add62_out1 = mul124_out1.add(mul125_out1);
        let concat52_out1 = burn::tensor::Tensor::cat(
            [add62_out1, slice98_out1].into(),
            3,
        );
        let slice101_out1 = transpose123_out1.clone().slice(s![.., .., .., 0..32]);
        let slice102_out1 = transpose123_out1.slice(s![.., .., .., 32..]);
        let mul126_out1 = slice101_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape185_out1 = slice101_out1.reshape([1, 24, 257, 2, 16]);
        let slice103_out1 = reshape185_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze51_out1 = slice103_out1.squeeze_dims::<4>(&[-2]);
        let slice104_out1 = reshape185_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze52_out1 = slice104_out1.squeeze_dims::<4>(&[-2]);
        let neg26_out1 = squeeze52_out1.neg();
        let concat53_out1 = burn::tensor::Tensor::cat(
            [neg26_out1, squeeze51_out1].into(),
            3,
        );
        let mul127_out1 = concat53_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add63_out1 = mul126_out1.add(mul127_out1);
        let concat54_out1 = burn::tensor::Tensor::cat(
            [add63_out1, slice102_out1].into(),
            3,
        );
        let reshape186_out1 = concat54_out1.reshape([-1, 257, 64]);
        let transpose125_out1 = reshape186_out1.permute([0, 2, 1]);
        let reshape187_out1 = transpose125_out1.reshape([1, 24, 64, 257]);
        let mul128_out1 = concat52_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul129_out1 = reshape187_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul138_out1 = mul128_out1.matmul(mul129_out1);
        let softmax25_out1 = burn::tensor::activation::softmax(matmul138_out1, 3);
        let matmul139_out1 = softmax25_out1.matmul(transpose124_out1);
        let transpose126_out1 = matmul139_out1.permute([0, 2, 1, 3]);
        let reshape188_out1 = transpose126_out1.reshape([1, 257, 1536]);
        let linear92_out1 = self.linear92.forward(reshape188_out1);
        let add64_out1 = add61_out1.add(linear92_out1);
        let layernormalization38_out1 = {
            let dtype = add64_out1.clone().dtype();
            self.layernormalization38
                .forward(add64_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear93_out1 = self.linear93.forward(layernormalization38_out1);
        let reshape189_out1 = linear93_out1.reshape([1, 257, 24, 64]);
        let transpose127_out1 = reshape189_out1.permute([0, 2, 1, 3]);
        let linear94_out1 = self.linear94.forward(linear2_out1.clone());
        let split_tensors = linear94_out1.split(768, 2);
        let [split38_out1, split38_out2] = split_tensors.try_into().unwrap();
        let reshape190_out1 = split38_out1.reshape([1, 130, 12, 64]);
        let transpose128_out1 = reshape190_out1.permute([0, 2, 1, 3]);
        let reshape191_out1 = split38_out2.reshape([1, 130, 12, 64]);
        let transpose129_out1 = reshape191_out1.permute([0, 2, 1, 3]);
        let unsqueeze27_out1: Tensor<B, 5> = transpose128_out1.unsqueeze_dims::<5>(&[2]);
        let expand25_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze27_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze27_out1.expand(shape)
        };
        let unsqueeze28_out1: Tensor<B, 5> = transpose129_out1.unsqueeze_dims::<5>(&[2]);
        let expand26_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze28_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze28_out1.expand(shape)
        };
        let reshape192_out1 = expand26_out1.reshape([1, -1, 130, 64]);
        let reshape193_out1 = expand25_out1.reshape([24, 130, 64]);
        let transpose130_out1 = reshape193_out1.permute([0, 2, 1]);
        let reshape194_out1 = transpose130_out1.reshape([1, 24, 64, 130]);
        let mul130_out1 = transpose127_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul131_out1 = reshape194_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul143_out1 = mul130_out1.matmul(mul131_out1);
        let softmax26_out1 = burn::tensor::activation::softmax(matmul143_out1, 3);
        let matmul144_out1 = softmax26_out1.matmul(reshape192_out1);
        let transpose131_out1 = matmul144_out1.permute([0, 2, 1, 3]);
        let reshape195_out1 = transpose131_out1.reshape([1, 257, 1536]);
        let linear95_out1 = self.linear95.forward(reshape195_out1);
        let add65_out1 = add64_out1.add(linear95_out1);
        let layernormalization39_out1 = {
            let dtype = add65_out1.clone().dtype();
            self.layernormalization39
                .forward(add65_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear96_out1 = self.linear96.forward(layernormalization39_out1);
        let split_tensors = linear96_out1.split(6144, 2);
        let [split39_out1, split39_out2] = split_tensors.try_into().unwrap();
        let sigmoid15_out1 = burn::tensor::activation::sigmoid(split39_out2.clone());
        let mul132_out1 = split39_out2.mul(sigmoid15_out1);
        let mul133_out1 = split39_out1.mul(mul132_out1);
        let linear97_out1 = self.linear97.forward(mul133_out1);
        let add66_out1 = add65_out1.add(linear97_out1);
        let layernormalization40_out1 = {
            let dtype = add66_out1.clone().dtype();
            self.layernormalization40
                .forward(add66_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear98_out1 = self.linear98.forward(layernormalization40_out1);
        let split_tensors = linear98_out1.split(1536, 2);
        let [split40_out1, split40_out2, split40_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape196_out1 = split40_out1.reshape([1, 257, 24, 64]);
        let transpose132_out1 = reshape196_out1.permute([0, 2, 1, 3]);
        let reshape197_out1 = split40_out2.reshape([1, 257, 24, 64]);
        let transpose133_out1 = reshape197_out1.permute([0, 2, 1, 3]);
        let reshape198_out1 = split40_out3.reshape([1, 257, 24, 64]);
        let transpose134_out1 = reshape198_out1.permute([0, 2, 1, 3]);
        let slice105_out1 = transpose132_out1.clone().slice(s![.., .., .., 0..32]);
        let slice106_out1 = transpose132_out1.slice(s![.., .., .., 32..]);
        let mul134_out1 = slice105_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape199_out1 = slice105_out1.reshape([1, 24, 257, 2, 16]);
        let slice107_out1 = reshape199_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze53_out1 = slice107_out1.squeeze_dims::<4>(&[-2]);
        let slice108_out1 = reshape199_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze54_out1 = slice108_out1.squeeze_dims::<4>(&[-2]);
        let neg27_out1 = squeeze54_out1.neg();
        let concat55_out1 = burn::tensor::Tensor::cat(
            [neg27_out1, squeeze53_out1].into(),
            3,
        );
        let mul135_out1 = concat55_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add67_out1 = mul134_out1.add(mul135_out1);
        let concat56_out1 = burn::tensor::Tensor::cat(
            [add67_out1, slice106_out1].into(),
            3,
        );
        let slice109_out1 = transpose133_out1.clone().slice(s![.., .., .., 0..32]);
        let slice110_out1 = transpose133_out1.slice(s![.., .., .., 32..]);
        let mul136_out1 = slice109_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape200_out1 = slice109_out1.reshape([1, 24, 257, 2, 16]);
        let slice111_out1 = reshape200_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze55_out1 = slice111_out1.squeeze_dims::<4>(&[-2]);
        let slice112_out1 = reshape200_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze56_out1 = slice112_out1.squeeze_dims::<4>(&[-2]);
        let neg28_out1 = squeeze56_out1.neg();
        let concat57_out1 = burn::tensor::Tensor::cat(
            [neg28_out1, squeeze55_out1].into(),
            3,
        );
        let mul137_out1 = concat57_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add68_out1 = mul136_out1.add(mul137_out1);
        let concat58_out1 = burn::tensor::Tensor::cat(
            [add68_out1, slice110_out1].into(),
            3,
        );
        let reshape201_out1 = concat58_out1.reshape([-1, 257, 64]);
        let transpose135_out1 = reshape201_out1.permute([0, 2, 1]);
        let reshape202_out1 = transpose135_out1.reshape([1, 24, 64, 257]);
        let mul138_out1 = concat56_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul139_out1 = reshape202_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul149_out1 = mul138_out1.matmul(mul139_out1);
        let softmax27_out1 = burn::tensor::activation::softmax(matmul149_out1, 3);
        let matmul150_out1 = softmax27_out1.matmul(transpose134_out1);
        let transpose136_out1 = matmul150_out1.permute([0, 2, 1, 3]);
        let reshape203_out1 = transpose136_out1.reshape([1, 257, 1536]);
        let linear99_out1 = self.linear99.forward(reshape203_out1);
        let add69_out1 = add66_out1.add(linear99_out1);
        let layernormalization41_out1 = {
            let dtype = add69_out1.clone().dtype();
            self.layernormalization41
                .forward(add69_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear100_out1 = self.linear100.forward(layernormalization41_out1);
        let reshape204_out1 = linear100_out1.reshape([1, 257, 24, 64]);
        let transpose137_out1 = reshape204_out1.permute([0, 2, 1, 3]);
        let linear101_out1 = self.linear101.forward(linear2_out1.clone());
        let split_tensors = linear101_out1.split(768, 2);
        let [split41_out1, split41_out2] = split_tensors.try_into().unwrap();
        let reshape205_out1 = split41_out1.reshape([1, 130, 12, 64]);
        let transpose138_out1 = reshape205_out1.permute([0, 2, 1, 3]);
        let reshape206_out1 = split41_out2.reshape([1, 130, 12, 64]);
        let transpose139_out1 = reshape206_out1.permute([0, 2, 1, 3]);
        let unsqueeze29_out1: Tensor<B, 5> = transpose138_out1.unsqueeze_dims::<5>(&[2]);
        let expand27_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze29_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze29_out1.expand(shape)
        };
        let unsqueeze30_out1: Tensor<B, 5> = transpose139_out1.unsqueeze_dims::<5>(&[2]);
        let expand28_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze30_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze30_out1.expand(shape)
        };
        let reshape207_out1 = expand28_out1.reshape([1, -1, 130, 64]);
        let reshape208_out1 = expand27_out1.reshape([24, 130, 64]);
        let transpose140_out1 = reshape208_out1.permute([0, 2, 1]);
        let reshape209_out1 = transpose140_out1.reshape([1, 24, 64, 130]);
        let mul140_out1 = transpose137_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul141_out1 = reshape209_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul154_out1 = mul140_out1.matmul(mul141_out1);
        let softmax28_out1 = burn::tensor::activation::softmax(matmul154_out1, 3);
        let matmul155_out1 = softmax28_out1.matmul(reshape207_out1);
        let transpose141_out1 = matmul155_out1.permute([0, 2, 1, 3]);
        let reshape210_out1 = transpose141_out1.reshape([1, 257, 1536]);
        let linear102_out1 = self.linear102.forward(reshape210_out1);
        let add70_out1 = add69_out1.add(linear102_out1);
        let layernormalization42_out1 = {
            let dtype = add70_out1.clone().dtype();
            self.layernormalization42
                .forward(add70_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear103_out1 = self.linear103.forward(layernormalization42_out1);
        let split_tensors = linear103_out1.split(6144, 2);
        let [split42_out1, split42_out2] = split_tensors.try_into().unwrap();
        let sigmoid16_out1 = burn::tensor::activation::sigmoid(split42_out2.clone());
        let mul142_out1 = split42_out2.mul(sigmoid16_out1);
        let mul143_out1 = split42_out1.mul(mul142_out1);
        let linear104_out1 = self.linear104.forward(mul143_out1);
        let add71_out1 = add70_out1.add(linear104_out1);
        let layernormalization43_out1 = {
            let dtype = add71_out1.clone().dtype();
            self.layernormalization43
                .forward(add71_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear105_out1 = self.linear105.forward(layernormalization43_out1);
        let split_tensors = linear105_out1.split(1536, 2);
        let [split43_out1, split43_out2, split43_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape211_out1 = split43_out1.reshape([1, 257, 24, 64]);
        let transpose142_out1 = reshape211_out1.permute([0, 2, 1, 3]);
        let reshape212_out1 = split43_out2.reshape([1, 257, 24, 64]);
        let transpose143_out1 = reshape212_out1.permute([0, 2, 1, 3]);
        let reshape213_out1 = split43_out3.reshape([1, 257, 24, 64]);
        let transpose144_out1 = reshape213_out1.permute([0, 2, 1, 3]);
        let slice113_out1 = transpose142_out1.clone().slice(s![.., .., .., 0..32]);
        let slice114_out1 = transpose142_out1.slice(s![.., .., .., 32..]);
        let mul144_out1 = slice113_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape214_out1 = slice113_out1.reshape([1, 24, 257, 2, 16]);
        let slice115_out1 = reshape214_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze57_out1 = slice115_out1.squeeze_dims::<4>(&[-2]);
        let slice116_out1 = reshape214_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze58_out1 = slice116_out1.squeeze_dims::<4>(&[-2]);
        let neg29_out1 = squeeze58_out1.neg();
        let concat59_out1 = burn::tensor::Tensor::cat(
            [neg29_out1, squeeze57_out1].into(),
            3,
        );
        let mul145_out1 = concat59_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add72_out1 = mul144_out1.add(mul145_out1);
        let concat60_out1 = burn::tensor::Tensor::cat(
            [add72_out1, slice114_out1].into(),
            3,
        );
        let slice117_out1 = transpose143_out1.clone().slice(s![.., .., .., 0..32]);
        let slice118_out1 = transpose143_out1.slice(s![.., .., .., 32..]);
        let mul146_out1 = slice117_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape215_out1 = slice117_out1.reshape([1, 24, 257, 2, 16]);
        let slice119_out1 = reshape215_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze59_out1 = slice119_out1.squeeze_dims::<4>(&[-2]);
        let slice120_out1 = reshape215_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze60_out1 = slice120_out1.squeeze_dims::<4>(&[-2]);
        let neg30_out1 = squeeze60_out1.neg();
        let concat61_out1 = burn::tensor::Tensor::cat(
            [neg30_out1, squeeze59_out1].into(),
            3,
        );
        let mul147_out1 = concat61_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add73_out1 = mul146_out1.add(mul147_out1);
        let concat62_out1 = burn::tensor::Tensor::cat(
            [add73_out1, slice118_out1].into(),
            3,
        );
        let reshape216_out1 = concat62_out1.reshape([-1, 257, 64]);
        let transpose145_out1 = reshape216_out1.permute([0, 2, 1]);
        let reshape217_out1 = transpose145_out1.reshape([1, 24, 64, 257]);
        let mul148_out1 = concat60_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul149_out1 = reshape217_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul160_out1 = mul148_out1.matmul(mul149_out1);
        let softmax29_out1 = burn::tensor::activation::softmax(matmul160_out1, 3);
        let matmul161_out1 = softmax29_out1.matmul(transpose144_out1);
        let transpose146_out1 = matmul161_out1.permute([0, 2, 1, 3]);
        let reshape218_out1 = transpose146_out1.reshape([1, 257, 1536]);
        let linear106_out1 = self.linear106.forward(reshape218_out1);
        let add74_out1 = add71_out1.add(linear106_out1);
        let layernormalization44_out1 = {
            let dtype = add74_out1.clone().dtype();
            self.layernormalization44
                .forward(add74_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear107_out1 = self.linear107.forward(layernormalization44_out1);
        let reshape219_out1 = linear107_out1.reshape([1, 257, 24, 64]);
        let transpose147_out1 = reshape219_out1.permute([0, 2, 1, 3]);
        let linear108_out1 = self.linear108.forward(linear2_out1.clone());
        let split_tensors = linear108_out1.split(768, 2);
        let [split44_out1, split44_out2] = split_tensors.try_into().unwrap();
        let reshape220_out1 = split44_out1.reshape([1, 130, 12, 64]);
        let transpose148_out1 = reshape220_out1.permute([0, 2, 1, 3]);
        let reshape221_out1 = split44_out2.reshape([1, 130, 12, 64]);
        let transpose149_out1 = reshape221_out1.permute([0, 2, 1, 3]);
        let unsqueeze31_out1: Tensor<B, 5> = transpose148_out1.unsqueeze_dims::<5>(&[2]);
        let expand29_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze31_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze31_out1.expand(shape)
        };
        let unsqueeze32_out1: Tensor<B, 5> = transpose149_out1.unsqueeze_dims::<5>(&[2]);
        let expand30_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze32_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze32_out1.expand(shape)
        };
        let reshape222_out1 = expand30_out1.reshape([1, -1, 130, 64]);
        let reshape223_out1 = expand29_out1.reshape([24, 130, 64]);
        let transpose150_out1 = reshape223_out1.permute([0, 2, 1]);
        let reshape224_out1 = transpose150_out1.reshape([1, 24, 64, 130]);
        let mul150_out1 = transpose147_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul151_out1 = reshape224_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul165_out1 = mul150_out1.matmul(mul151_out1);
        let softmax30_out1 = burn::tensor::activation::softmax(matmul165_out1, 3);
        let matmul166_out1 = softmax30_out1.matmul(reshape222_out1);
        let transpose151_out1 = matmul166_out1.permute([0, 2, 1, 3]);
        let reshape225_out1 = transpose151_out1.reshape([1, 257, 1536]);
        let linear109_out1 = self.linear109.forward(reshape225_out1);
        let add75_out1 = add74_out1.add(linear109_out1);
        let layernormalization45_out1 = {
            let dtype = add75_out1.clone().dtype();
            self.layernormalization45
                .forward(add75_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear110_out1 = self.linear110.forward(layernormalization45_out1);
        let split_tensors = linear110_out1.split(6144, 2);
        let [split45_out1, split45_out2] = split_tensors.try_into().unwrap();
        let sigmoid17_out1 = burn::tensor::activation::sigmoid(split45_out2.clone());
        let mul152_out1 = split45_out2.mul(sigmoid17_out1);
        let mul153_out1 = split45_out1.mul(mul152_out1);
        let linear111_out1 = self.linear111.forward(mul153_out1);
        let add76_out1 = add75_out1.add(linear111_out1);
        let layernormalization46_out1 = {
            let dtype = add76_out1.clone().dtype();
            self.layernormalization46
                .forward(add76_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear112_out1 = self.linear112.forward(layernormalization46_out1);
        let split_tensors = linear112_out1.split(1536, 2);
        let [split46_out1, split46_out2, split46_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape226_out1 = split46_out1.reshape([1, 257, 24, 64]);
        let transpose152_out1 = reshape226_out1.permute([0, 2, 1, 3]);
        let reshape227_out1 = split46_out2.reshape([1, 257, 24, 64]);
        let transpose153_out1 = reshape227_out1.permute([0, 2, 1, 3]);
        let reshape228_out1 = split46_out3.reshape([1, 257, 24, 64]);
        let transpose154_out1 = reshape228_out1.permute([0, 2, 1, 3]);
        let slice121_out1 = transpose152_out1.clone().slice(s![.., .., .., 0..32]);
        let slice122_out1 = transpose152_out1.slice(s![.., .., .., 32..]);
        let mul154_out1 = slice121_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape229_out1 = slice121_out1.reshape([1, 24, 257, 2, 16]);
        let slice123_out1 = reshape229_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze61_out1 = slice123_out1.squeeze_dims::<4>(&[-2]);
        let slice124_out1 = reshape229_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze62_out1 = slice124_out1.squeeze_dims::<4>(&[-2]);
        let neg31_out1 = squeeze62_out1.neg();
        let concat63_out1 = burn::tensor::Tensor::cat(
            [neg31_out1, squeeze61_out1].into(),
            3,
        );
        let mul155_out1 = concat63_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add77_out1 = mul154_out1.add(mul155_out1);
        let concat64_out1 = burn::tensor::Tensor::cat(
            [add77_out1, slice122_out1].into(),
            3,
        );
        let slice125_out1 = transpose153_out1.clone().slice(s![.., .., .., 0..32]);
        let slice126_out1 = transpose153_out1.slice(s![.., .., .., 32..]);
        let mul156_out1 = slice125_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape230_out1 = slice125_out1.reshape([1, 24, 257, 2, 16]);
        let slice127_out1 = reshape230_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze63_out1 = slice127_out1.squeeze_dims::<4>(&[-2]);
        let slice128_out1 = reshape230_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze64_out1 = slice128_out1.squeeze_dims::<4>(&[-2]);
        let neg32_out1 = squeeze64_out1.neg();
        let concat65_out1 = burn::tensor::Tensor::cat(
            [neg32_out1, squeeze63_out1].into(),
            3,
        );
        let mul157_out1 = concat65_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add78_out1 = mul156_out1.add(mul157_out1);
        let concat66_out1 = burn::tensor::Tensor::cat(
            [add78_out1, slice126_out1].into(),
            3,
        );
        let reshape231_out1 = concat66_out1.reshape([-1, 257, 64]);
        let transpose155_out1 = reshape231_out1.permute([0, 2, 1]);
        let reshape232_out1 = transpose155_out1.reshape([1, 24, 64, 257]);
        let mul158_out1 = concat64_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul159_out1 = reshape232_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul171_out1 = mul158_out1.matmul(mul159_out1);
        let softmax31_out1 = burn::tensor::activation::softmax(matmul171_out1, 3);
        let matmul172_out1 = softmax31_out1.matmul(transpose154_out1);
        let transpose156_out1 = matmul172_out1.permute([0, 2, 1, 3]);
        let reshape233_out1 = transpose156_out1.reshape([1, 257, 1536]);
        let linear113_out1 = self.linear113.forward(reshape233_out1);
        let add79_out1 = add76_out1.add(linear113_out1);
        let layernormalization47_out1 = {
            let dtype = add79_out1.clone().dtype();
            self.layernormalization47
                .forward(add79_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear114_out1 = self.linear114.forward(layernormalization47_out1);
        let reshape234_out1 = linear114_out1.reshape([1, 257, 24, 64]);
        let transpose157_out1 = reshape234_out1.permute([0, 2, 1, 3]);
        let linear115_out1 = self.linear115.forward(linear2_out1.clone());
        let split_tensors = linear115_out1.split(768, 2);
        let [split47_out1, split47_out2] = split_tensors.try_into().unwrap();
        let reshape235_out1 = split47_out1.reshape([1, 130, 12, 64]);
        let transpose158_out1 = reshape235_out1.permute([0, 2, 1, 3]);
        let reshape236_out1 = split47_out2.reshape([1, 130, 12, 64]);
        let transpose159_out1 = reshape236_out1.permute([0, 2, 1, 3]);
        let unsqueeze33_out1: Tensor<B, 5> = transpose158_out1.unsqueeze_dims::<5>(&[2]);
        let expand31_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze33_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze33_out1.expand(shape)
        };
        let unsqueeze34_out1: Tensor<B, 5> = transpose159_out1.unsqueeze_dims::<5>(&[2]);
        let expand32_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze34_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze34_out1.expand(shape)
        };
        let reshape237_out1 = expand32_out1.reshape([1, -1, 130, 64]);
        let reshape238_out1 = expand31_out1.reshape([24, 130, 64]);
        let transpose160_out1 = reshape238_out1.permute([0, 2, 1]);
        let reshape239_out1 = transpose160_out1.reshape([1, 24, 64, 130]);
        let mul160_out1 = transpose157_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul161_out1 = reshape239_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul176_out1 = mul160_out1.matmul(mul161_out1);
        let softmax32_out1 = burn::tensor::activation::softmax(matmul176_out1, 3);
        let matmul177_out1 = softmax32_out1.matmul(reshape237_out1);
        let transpose161_out1 = matmul177_out1.permute([0, 2, 1, 3]);
        let reshape240_out1 = transpose161_out1.reshape([1, 257, 1536]);
        let linear116_out1 = self.linear116.forward(reshape240_out1);
        let add80_out1 = add79_out1.add(linear116_out1);
        let layernormalization48_out1 = {
            let dtype = add80_out1.clone().dtype();
            self.layernormalization48
                .forward(add80_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear117_out1 = self.linear117.forward(layernormalization48_out1);
        let split_tensors = linear117_out1.split(6144, 2);
        let [split48_out1, split48_out2] = split_tensors.try_into().unwrap();
        let sigmoid18_out1 = burn::tensor::activation::sigmoid(split48_out2.clone());
        let mul162_out1 = split48_out2.mul(sigmoid18_out1);
        let mul163_out1 = split48_out1.mul(mul162_out1);
        let linear118_out1 = self.linear118.forward(mul163_out1);
        let add81_out1 = add80_out1.add(linear118_out1);
        let layernormalization49_out1 = {
            let dtype = add81_out1.clone().dtype();
            self.layernormalization49
                .forward(add81_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear119_out1 = self.linear119.forward(layernormalization49_out1);
        let split_tensors = linear119_out1.split(1536, 2);
        let [split49_out1, split49_out2, split49_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape241_out1 = split49_out1.reshape([1, 257, 24, 64]);
        let transpose162_out1 = reshape241_out1.permute([0, 2, 1, 3]);
        let reshape242_out1 = split49_out2.reshape([1, 257, 24, 64]);
        let transpose163_out1 = reshape242_out1.permute([0, 2, 1, 3]);
        let reshape243_out1 = split49_out3.reshape([1, 257, 24, 64]);
        let transpose164_out1 = reshape243_out1.permute([0, 2, 1, 3]);
        let slice129_out1 = transpose162_out1.clone().slice(s![.., .., .., 0..32]);
        let slice130_out1 = transpose162_out1.slice(s![.., .., .., 32..]);
        let mul164_out1 = slice129_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape244_out1 = slice129_out1.reshape([1, 24, 257, 2, 16]);
        let slice131_out1 = reshape244_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze65_out1 = slice131_out1.squeeze_dims::<4>(&[-2]);
        let slice132_out1 = reshape244_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze66_out1 = slice132_out1.squeeze_dims::<4>(&[-2]);
        let neg33_out1 = squeeze66_out1.neg();
        let concat67_out1 = burn::tensor::Tensor::cat(
            [neg33_out1, squeeze65_out1].into(),
            3,
        );
        let mul165_out1 = concat67_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add82_out1 = mul164_out1.add(mul165_out1);
        let concat68_out1 = burn::tensor::Tensor::cat(
            [add82_out1, slice130_out1].into(),
            3,
        );
        let slice133_out1 = transpose163_out1.clone().slice(s![.., .., .., 0..32]);
        let slice134_out1 = transpose163_out1.slice(s![.., .., .., 32..]);
        let mul166_out1 = slice133_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape245_out1 = slice133_out1.reshape([1, 24, 257, 2, 16]);
        let slice135_out1 = reshape245_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze67_out1 = slice135_out1.squeeze_dims::<4>(&[-2]);
        let slice136_out1 = reshape245_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze68_out1 = slice136_out1.squeeze_dims::<4>(&[-2]);
        let neg34_out1 = squeeze68_out1.neg();
        let concat69_out1 = burn::tensor::Tensor::cat(
            [neg34_out1, squeeze67_out1].into(),
            3,
        );
        let mul167_out1 = concat69_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add83_out1 = mul166_out1.add(mul167_out1);
        let concat70_out1 = burn::tensor::Tensor::cat(
            [add83_out1, slice134_out1].into(),
            3,
        );
        let reshape246_out1 = concat70_out1.reshape([-1, 257, 64]);
        let transpose165_out1 = reshape246_out1.permute([0, 2, 1]);
        let reshape247_out1 = transpose165_out1.reshape([1, 24, 64, 257]);
        let mul168_out1 = concat68_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul169_out1 = reshape247_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul182_out1 = mul168_out1.matmul(mul169_out1);
        let softmax33_out1 = burn::tensor::activation::softmax(matmul182_out1, 3);
        let matmul183_out1 = softmax33_out1.matmul(transpose164_out1);
        let transpose166_out1 = matmul183_out1.permute([0, 2, 1, 3]);
        let reshape248_out1 = transpose166_out1.reshape([1, 257, 1536]);
        let linear120_out1 = self.linear120.forward(reshape248_out1);
        let add84_out1 = add81_out1.add(linear120_out1);
        let layernormalization50_out1 = {
            let dtype = add84_out1.clone().dtype();
            self.layernormalization50
                .forward(add84_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear121_out1 = self.linear121.forward(layernormalization50_out1);
        let reshape249_out1 = linear121_out1.reshape([1, 257, 24, 64]);
        let transpose167_out1 = reshape249_out1.permute([0, 2, 1, 3]);
        let linear122_out1 = self.linear122.forward(linear2_out1.clone());
        let split_tensors = linear122_out1.split(768, 2);
        let [split50_out1, split50_out2] = split_tensors.try_into().unwrap();
        let reshape250_out1 = split50_out1.reshape([1, 130, 12, 64]);
        let transpose168_out1 = reshape250_out1.permute([0, 2, 1, 3]);
        let reshape251_out1 = split50_out2.reshape([1, 130, 12, 64]);
        let transpose169_out1 = reshape251_out1.permute([0, 2, 1, 3]);
        let unsqueeze35_out1: Tensor<B, 5> = transpose168_out1.unsqueeze_dims::<5>(&[2]);
        let expand33_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze35_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze35_out1.expand(shape)
        };
        let unsqueeze36_out1: Tensor<B, 5> = transpose169_out1.unsqueeze_dims::<5>(&[2]);
        let expand34_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze36_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze36_out1.expand(shape)
        };
        let reshape252_out1 = expand34_out1.reshape([1, -1, 130, 64]);
        let reshape253_out1 = expand33_out1.reshape([24, 130, 64]);
        let transpose170_out1 = reshape253_out1.permute([0, 2, 1]);
        let reshape254_out1 = transpose170_out1.reshape([1, 24, 64, 130]);
        let mul170_out1 = transpose167_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul171_out1 = reshape254_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul187_out1 = mul170_out1.matmul(mul171_out1);
        let softmax34_out1 = burn::tensor::activation::softmax(matmul187_out1, 3);
        let matmul188_out1 = softmax34_out1.matmul(reshape252_out1);
        let transpose171_out1 = matmul188_out1.permute([0, 2, 1, 3]);
        let reshape255_out1 = transpose171_out1.reshape([1, 257, 1536]);
        let linear123_out1 = self.linear123.forward(reshape255_out1);
        let add85_out1 = add84_out1.add(linear123_out1);
        let layernormalization51_out1 = {
            let dtype = add85_out1.clone().dtype();
            self.layernormalization51
                .forward(add85_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear124_out1 = self.linear124.forward(layernormalization51_out1);
        let split_tensors = linear124_out1.split(6144, 2);
        let [split51_out1, split51_out2] = split_tensors.try_into().unwrap();
        let sigmoid19_out1 = burn::tensor::activation::sigmoid(split51_out2.clone());
        let mul172_out1 = split51_out2.mul(sigmoid19_out1);
        let mul173_out1 = split51_out1.mul(mul172_out1);
        let linear125_out1 = self.linear125.forward(mul173_out1);
        let add86_out1 = add85_out1.add(linear125_out1);
        let layernormalization52_out1 = {
            let dtype = add86_out1.clone().dtype();
            self.layernormalization52
                .forward(add86_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear126_out1 = self.linear126.forward(layernormalization52_out1);
        let split_tensors = linear126_out1.split(1536, 2);
        let [split52_out1, split52_out2, split52_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape256_out1 = split52_out1.reshape([1, 257, 24, 64]);
        let transpose172_out1 = reshape256_out1.permute([0, 2, 1, 3]);
        let reshape257_out1 = split52_out2.reshape([1, 257, 24, 64]);
        let transpose173_out1 = reshape257_out1.permute([0, 2, 1, 3]);
        let reshape258_out1 = split52_out3.reshape([1, 257, 24, 64]);
        let transpose174_out1 = reshape258_out1.permute([0, 2, 1, 3]);
        let slice137_out1 = transpose172_out1.clone().slice(s![.., .., .., 0..32]);
        let slice138_out1 = transpose172_out1.slice(s![.., .., .., 32..]);
        let mul174_out1 = slice137_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape259_out1 = slice137_out1.reshape([1, 24, 257, 2, 16]);
        let slice139_out1 = reshape259_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze69_out1 = slice139_out1.squeeze_dims::<4>(&[-2]);
        let slice140_out1 = reshape259_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze70_out1 = slice140_out1.squeeze_dims::<4>(&[-2]);
        let neg35_out1 = squeeze70_out1.neg();
        let concat71_out1 = burn::tensor::Tensor::cat(
            [neg35_out1, squeeze69_out1].into(),
            3,
        );
        let mul175_out1 = concat71_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add87_out1 = mul174_out1.add(mul175_out1);
        let concat72_out1 = burn::tensor::Tensor::cat(
            [add87_out1, slice138_out1].into(),
            3,
        );
        let slice141_out1 = transpose173_out1.clone().slice(s![.., .., .., 0..32]);
        let slice142_out1 = transpose173_out1.slice(s![.., .., .., 32..]);
        let mul176_out1 = slice141_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape260_out1 = slice141_out1.reshape([1, 24, 257, 2, 16]);
        let slice143_out1 = reshape260_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze71_out1 = slice143_out1.squeeze_dims::<4>(&[-2]);
        let slice144_out1 = reshape260_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze72_out1 = slice144_out1.squeeze_dims::<4>(&[-2]);
        let neg36_out1 = squeeze72_out1.neg();
        let concat73_out1 = burn::tensor::Tensor::cat(
            [neg36_out1, squeeze71_out1].into(),
            3,
        );
        let mul177_out1 = concat73_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add88_out1 = mul176_out1.add(mul177_out1);
        let concat74_out1 = burn::tensor::Tensor::cat(
            [add88_out1, slice142_out1].into(),
            3,
        );
        let reshape261_out1 = concat74_out1.reshape([-1, 257, 64]);
        let transpose175_out1 = reshape261_out1.permute([0, 2, 1]);
        let reshape262_out1 = transpose175_out1.reshape([1, 24, 64, 257]);
        let mul178_out1 = concat72_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul179_out1 = reshape262_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul193_out1 = mul178_out1.matmul(mul179_out1);
        let softmax35_out1 = burn::tensor::activation::softmax(matmul193_out1, 3);
        let matmul194_out1 = softmax35_out1.matmul(transpose174_out1);
        let transpose176_out1 = matmul194_out1.permute([0, 2, 1, 3]);
        let reshape263_out1 = transpose176_out1.reshape([1, 257, 1536]);
        let linear127_out1 = self.linear127.forward(reshape263_out1);
        let add89_out1 = add86_out1.add(linear127_out1);
        let layernormalization53_out1 = {
            let dtype = add89_out1.clone().dtype();
            self.layernormalization53
                .forward(add89_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear128_out1 = self.linear128.forward(layernormalization53_out1);
        let reshape264_out1 = linear128_out1.reshape([1, 257, 24, 64]);
        let transpose177_out1 = reshape264_out1.permute([0, 2, 1, 3]);
        let linear129_out1 = self.linear129.forward(linear2_out1.clone());
        let split_tensors = linear129_out1.split(768, 2);
        let [split53_out1, split53_out2] = split_tensors.try_into().unwrap();
        let reshape265_out1 = split53_out1.reshape([1, 130, 12, 64]);
        let transpose178_out1 = reshape265_out1.permute([0, 2, 1, 3]);
        let reshape266_out1 = split53_out2.reshape([1, 130, 12, 64]);
        let transpose179_out1 = reshape266_out1.permute([0, 2, 1, 3]);
        let unsqueeze37_out1: Tensor<B, 5> = transpose178_out1.unsqueeze_dims::<5>(&[2]);
        let expand35_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze37_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze37_out1.expand(shape)
        };
        let unsqueeze38_out1: Tensor<B, 5> = transpose179_out1.unsqueeze_dims::<5>(&[2]);
        let expand36_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze38_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze38_out1.expand(shape)
        };
        let reshape267_out1 = expand36_out1.reshape([1, -1, 130, 64]);
        let reshape268_out1 = expand35_out1.reshape([24, 130, 64]);
        let transpose180_out1 = reshape268_out1.permute([0, 2, 1]);
        let reshape269_out1 = transpose180_out1.reshape([1, 24, 64, 130]);
        let mul180_out1 = transpose177_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul181_out1 = reshape269_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul198_out1 = mul180_out1.matmul(mul181_out1);
        let softmax36_out1 = burn::tensor::activation::softmax(matmul198_out1, 3);
        let matmul199_out1 = softmax36_out1.matmul(reshape267_out1);
        let transpose181_out1 = matmul199_out1.permute([0, 2, 1, 3]);
        let reshape270_out1 = transpose181_out1.reshape([1, 257, 1536]);
        let linear130_out1 = self.linear130.forward(reshape270_out1);
        let add90_out1 = add89_out1.add(linear130_out1);
        let layernormalization54_out1 = {
            let dtype = add90_out1.clone().dtype();
            self.layernormalization54
                .forward(add90_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear131_out1 = self.linear131.forward(layernormalization54_out1);
        let split_tensors = linear131_out1.split(6144, 2);
        let [split54_out1, split54_out2] = split_tensors.try_into().unwrap();
        let sigmoid20_out1 = burn::tensor::activation::sigmoid(split54_out2.clone());
        let mul182_out1 = split54_out2.mul(sigmoid20_out1);
        let mul183_out1 = split54_out1.mul(mul182_out1);
        let linear132_out1 = self.linear132.forward(mul183_out1);
        let add91_out1 = add90_out1.add(linear132_out1);
        let layernormalization55_out1 = {
            let dtype = add91_out1.clone().dtype();
            self.layernormalization55
                .forward(add91_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear133_out1 = self.linear133.forward(layernormalization55_out1);
        let split_tensors = linear133_out1.split(1536, 2);
        let [split55_out1, split55_out2, split55_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape271_out1 = split55_out1.reshape([1, 257, 24, 64]);
        let transpose182_out1 = reshape271_out1.permute([0, 2, 1, 3]);
        let reshape272_out1 = split55_out2.reshape([1, 257, 24, 64]);
        let transpose183_out1 = reshape272_out1.permute([0, 2, 1, 3]);
        let reshape273_out1 = split55_out3.reshape([1, 257, 24, 64]);
        let transpose184_out1 = reshape273_out1.permute([0, 2, 1, 3]);
        let slice145_out1 = transpose182_out1.clone().slice(s![.., .., .., 0..32]);
        let slice146_out1 = transpose182_out1.slice(s![.., .., .., 32..]);
        let mul184_out1 = slice145_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape274_out1 = slice145_out1.reshape([1, 24, 257, 2, 16]);
        let slice147_out1 = reshape274_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze73_out1 = slice147_out1.squeeze_dims::<4>(&[-2]);
        let slice148_out1 = reshape274_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze74_out1 = slice148_out1.squeeze_dims::<4>(&[-2]);
        let neg37_out1 = squeeze74_out1.neg();
        let concat75_out1 = burn::tensor::Tensor::cat(
            [neg37_out1, squeeze73_out1].into(),
            3,
        );
        let mul185_out1 = concat75_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add92_out1 = mul184_out1.add(mul185_out1);
        let concat76_out1 = burn::tensor::Tensor::cat(
            [add92_out1, slice146_out1].into(),
            3,
        );
        let slice149_out1 = transpose183_out1.clone().slice(s![.., .., .., 0..32]);
        let slice150_out1 = transpose183_out1.slice(s![.., .., .., 32..]);
        let mul186_out1 = slice149_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape275_out1 = slice149_out1.reshape([1, 24, 257, 2, 16]);
        let slice151_out1 = reshape275_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze75_out1 = slice151_out1.squeeze_dims::<4>(&[-2]);
        let slice152_out1 = reshape275_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze76_out1 = slice152_out1.squeeze_dims::<4>(&[-2]);
        let neg38_out1 = squeeze76_out1.neg();
        let concat77_out1 = burn::tensor::Tensor::cat(
            [neg38_out1, squeeze75_out1].into(),
            3,
        );
        let mul187_out1 = concat77_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add93_out1 = mul186_out1.add(mul187_out1);
        let concat78_out1 = burn::tensor::Tensor::cat(
            [add93_out1, slice150_out1].into(),
            3,
        );
        let reshape276_out1 = concat78_out1.reshape([-1, 257, 64]);
        let transpose185_out1 = reshape276_out1.permute([0, 2, 1]);
        let reshape277_out1 = transpose185_out1.reshape([1, 24, 64, 257]);
        let mul188_out1 = concat76_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul189_out1 = reshape277_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul204_out1 = mul188_out1.matmul(mul189_out1);
        let softmax37_out1 = burn::tensor::activation::softmax(matmul204_out1, 3);
        let matmul205_out1 = softmax37_out1.matmul(transpose184_out1);
        let transpose186_out1 = matmul205_out1.permute([0, 2, 1, 3]);
        let reshape278_out1 = transpose186_out1.reshape([1, 257, 1536]);
        let linear134_out1 = self.linear134.forward(reshape278_out1);
        let add94_out1 = add91_out1.add(linear134_out1);
        let layernormalization56_out1 = {
            let dtype = add94_out1.clone().dtype();
            self.layernormalization56
                .forward(add94_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear135_out1 = self.linear135.forward(layernormalization56_out1);
        let reshape279_out1 = linear135_out1.reshape([1, 257, 24, 64]);
        let transpose187_out1 = reshape279_out1.permute([0, 2, 1, 3]);
        let linear136_out1 = self.linear136.forward(linear2_out1.clone());
        let split_tensors = linear136_out1.split(768, 2);
        let [split56_out1, split56_out2] = split_tensors.try_into().unwrap();
        let reshape280_out1 = split56_out1.reshape([1, 130, 12, 64]);
        let transpose188_out1 = reshape280_out1.permute([0, 2, 1, 3]);
        let reshape281_out1 = split56_out2.reshape([1, 130, 12, 64]);
        let transpose189_out1 = reshape281_out1.permute([0, 2, 1, 3]);
        let unsqueeze39_out1: Tensor<B, 5> = transpose188_out1.unsqueeze_dims::<5>(&[2]);
        let expand37_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze39_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze39_out1.expand(shape)
        };
        let unsqueeze40_out1: Tensor<B, 5> = transpose189_out1.unsqueeze_dims::<5>(&[2]);
        let expand38_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze40_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze40_out1.expand(shape)
        };
        let reshape282_out1 = expand38_out1.reshape([1, -1, 130, 64]);
        let reshape283_out1 = expand37_out1.reshape([24, 130, 64]);
        let transpose190_out1 = reshape283_out1.permute([0, 2, 1]);
        let reshape284_out1 = transpose190_out1.reshape([1, 24, 64, 130]);
        let mul190_out1 = transpose187_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul191_out1 = reshape284_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul209_out1 = mul190_out1.matmul(mul191_out1);
        let softmax38_out1 = burn::tensor::activation::softmax(matmul209_out1, 3);
        let matmul210_out1 = softmax38_out1.matmul(reshape282_out1);
        let transpose191_out1 = matmul210_out1.permute([0, 2, 1, 3]);
        let reshape285_out1 = transpose191_out1.reshape([1, 257, 1536]);
        let linear137_out1 = self.linear137.forward(reshape285_out1);
        let add95_out1 = add94_out1.add(linear137_out1);
        let layernormalization57_out1 = {
            let dtype = add95_out1.clone().dtype();
            self.layernormalization57
                .forward(add95_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear138_out1 = self.linear138.forward(layernormalization57_out1);
        let split_tensors = linear138_out1.split(6144, 2);
        let [split57_out1, split57_out2] = split_tensors.try_into().unwrap();
        let sigmoid21_out1 = burn::tensor::activation::sigmoid(split57_out2.clone());
        let mul192_out1 = split57_out2.mul(sigmoid21_out1);
        let mul193_out1 = split57_out1.mul(mul192_out1);
        let linear139_out1 = self.linear139.forward(mul193_out1);
        let add96_out1 = add95_out1.add(linear139_out1);
        let layernormalization58_out1 = {
            let dtype = add96_out1.clone().dtype();
            self.layernormalization58
                .forward(add96_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear140_out1 = self.linear140.forward(layernormalization58_out1);
        let split_tensors = linear140_out1.split(1536, 2);
        let [split58_out1, split58_out2, split58_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape286_out1 = split58_out1.reshape([1, 257, 24, 64]);
        let transpose192_out1 = reshape286_out1.permute([0, 2, 1, 3]);
        let reshape287_out1 = split58_out2.reshape([1, 257, 24, 64]);
        let transpose193_out1 = reshape287_out1.permute([0, 2, 1, 3]);
        let reshape288_out1 = split58_out3.reshape([1, 257, 24, 64]);
        let transpose194_out1 = reshape288_out1.permute([0, 2, 1, 3]);
        let slice153_out1 = transpose192_out1.clone().slice(s![.., .., .., 0..32]);
        let slice154_out1 = transpose192_out1.slice(s![.., .., .., 32..]);
        let mul194_out1 = slice153_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape289_out1 = slice153_out1.reshape([1, 24, 257, 2, 16]);
        let slice155_out1 = reshape289_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze77_out1 = slice155_out1.squeeze_dims::<4>(&[-2]);
        let slice156_out1 = reshape289_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze78_out1 = slice156_out1.squeeze_dims::<4>(&[-2]);
        let neg39_out1 = squeeze78_out1.neg();
        let concat79_out1 = burn::tensor::Tensor::cat(
            [neg39_out1, squeeze77_out1].into(),
            3,
        );
        let mul195_out1 = concat79_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add97_out1 = mul194_out1.add(mul195_out1);
        let concat80_out1 = burn::tensor::Tensor::cat(
            [add97_out1, slice154_out1].into(),
            3,
        );
        let slice157_out1 = transpose193_out1.clone().slice(s![.., .., .., 0..32]);
        let slice158_out1 = transpose193_out1.slice(s![.., .., .., 32..]);
        let mul196_out1 = slice157_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape290_out1 = slice157_out1.reshape([1, 24, 257, 2, 16]);
        let slice159_out1 = reshape290_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze79_out1 = slice159_out1.squeeze_dims::<4>(&[-2]);
        let slice160_out1 = reshape290_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze80_out1 = slice160_out1.squeeze_dims::<4>(&[-2]);
        let neg40_out1 = squeeze80_out1.neg();
        let concat81_out1 = burn::tensor::Tensor::cat(
            [neg40_out1, squeeze79_out1].into(),
            3,
        );
        let mul197_out1 = concat81_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add98_out1 = mul196_out1.add(mul197_out1);
        let concat82_out1 = burn::tensor::Tensor::cat(
            [add98_out1, slice158_out1].into(),
            3,
        );
        let reshape291_out1 = concat82_out1.reshape([-1, 257, 64]);
        let transpose195_out1 = reshape291_out1.permute([0, 2, 1]);
        let reshape292_out1 = transpose195_out1.reshape([1, 24, 64, 257]);
        let mul198_out1 = concat80_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul199_out1 = reshape292_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul215_out1 = mul198_out1.matmul(mul199_out1);
        let softmax39_out1 = burn::tensor::activation::softmax(matmul215_out1, 3);
        let matmul216_out1 = softmax39_out1.matmul(transpose194_out1);
        let transpose196_out1 = matmul216_out1.permute([0, 2, 1, 3]);
        let reshape293_out1 = transpose196_out1.reshape([1, 257, 1536]);
        let linear141_out1 = self.linear141.forward(reshape293_out1);
        let add99_out1 = add96_out1.add(linear141_out1);
        let layernormalization59_out1 = {
            let dtype = add99_out1.clone().dtype();
            self.layernormalization59
                .forward(add99_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear142_out1 = self.linear142.forward(layernormalization59_out1);
        let reshape294_out1 = linear142_out1.reshape([1, 257, 24, 64]);
        let transpose197_out1 = reshape294_out1.permute([0, 2, 1, 3]);
        let linear143_out1 = self.linear143.forward(linear2_out1.clone());
        let split_tensors = linear143_out1.split(768, 2);
        let [split59_out1, split59_out2] = split_tensors.try_into().unwrap();
        let reshape295_out1 = split59_out1.reshape([1, 130, 12, 64]);
        let transpose198_out1 = reshape295_out1.permute([0, 2, 1, 3]);
        let reshape296_out1 = split59_out2.reshape([1, 130, 12, 64]);
        let transpose199_out1 = reshape296_out1.permute([0, 2, 1, 3]);
        let unsqueeze41_out1: Tensor<B, 5> = transpose198_out1.unsqueeze_dims::<5>(&[2]);
        let expand39_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze41_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze41_out1.expand(shape)
        };
        let unsqueeze42_out1: Tensor<B, 5> = transpose199_out1.unsqueeze_dims::<5>(&[2]);
        let expand40_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze42_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze42_out1.expand(shape)
        };
        let reshape297_out1 = expand40_out1.reshape([1, -1, 130, 64]);
        let reshape298_out1 = expand39_out1.reshape([24, 130, 64]);
        let transpose200_out1 = reshape298_out1.permute([0, 2, 1]);
        let reshape299_out1 = transpose200_out1.reshape([1, 24, 64, 130]);
        let mul200_out1 = transpose197_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul201_out1 = reshape299_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul220_out1 = mul200_out1.matmul(mul201_out1);
        let softmax40_out1 = burn::tensor::activation::softmax(matmul220_out1, 3);
        let matmul221_out1 = softmax40_out1.matmul(reshape297_out1);
        let transpose201_out1 = matmul221_out1.permute([0, 2, 1, 3]);
        let reshape300_out1 = transpose201_out1.reshape([1, 257, 1536]);
        let linear144_out1 = self.linear144.forward(reshape300_out1);
        let add100_out1 = add99_out1.add(linear144_out1);
        let layernormalization60_out1 = {
            let dtype = add100_out1.clone().dtype();
            self.layernormalization60
                .forward(add100_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear145_out1 = self.linear145.forward(layernormalization60_out1);
        let split_tensors = linear145_out1.split(6144, 2);
        let [split60_out1, split60_out2] = split_tensors.try_into().unwrap();
        let sigmoid22_out1 = burn::tensor::activation::sigmoid(split60_out2.clone());
        let mul202_out1 = split60_out2.mul(sigmoid22_out1);
        let mul203_out1 = split60_out1.mul(mul202_out1);
        let linear146_out1 = self.linear146.forward(mul203_out1);
        let add101_out1 = add100_out1.add(linear146_out1);
        let layernormalization61_out1 = {
            let dtype = add101_out1.clone().dtype();
            self.layernormalization61
                .forward(add101_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear147_out1 = self.linear147.forward(layernormalization61_out1);
        let split_tensors = linear147_out1.split(1536, 2);
        let [split61_out1, split61_out2, split61_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape301_out1 = split61_out1.reshape([1, 257, 24, 64]);
        let transpose202_out1 = reshape301_out1.permute([0, 2, 1, 3]);
        let reshape302_out1 = split61_out2.reshape([1, 257, 24, 64]);
        let transpose203_out1 = reshape302_out1.permute([0, 2, 1, 3]);
        let reshape303_out1 = split61_out3.reshape([1, 257, 24, 64]);
        let transpose204_out1 = reshape303_out1.permute([0, 2, 1, 3]);
        let slice161_out1 = transpose202_out1.clone().slice(s![.., .., .., 0..32]);
        let slice162_out1 = transpose202_out1.slice(s![.., .., .., 32..]);
        let mul204_out1 = slice161_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape304_out1 = slice161_out1.reshape([1, 24, 257, 2, 16]);
        let slice163_out1 = reshape304_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze81_out1 = slice163_out1.squeeze_dims::<4>(&[-2]);
        let slice164_out1 = reshape304_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze82_out1 = slice164_out1.squeeze_dims::<4>(&[-2]);
        let neg41_out1 = squeeze82_out1.neg();
        let concat83_out1 = burn::tensor::Tensor::cat(
            [neg41_out1, squeeze81_out1].into(),
            3,
        );
        let mul205_out1 = concat83_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add102_out1 = mul204_out1.add(mul205_out1);
        let concat84_out1 = burn::tensor::Tensor::cat(
            [add102_out1, slice162_out1].into(),
            3,
        );
        let slice165_out1 = transpose203_out1.clone().slice(s![.., .., .., 0..32]);
        let slice166_out1 = transpose203_out1.slice(s![.., .., .., 32..]);
        let mul206_out1 = slice165_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape305_out1 = slice165_out1.reshape([1, 24, 257, 2, 16]);
        let slice167_out1 = reshape305_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze83_out1 = slice167_out1.squeeze_dims::<4>(&[-2]);
        let slice168_out1 = reshape305_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze84_out1 = slice168_out1.squeeze_dims::<4>(&[-2]);
        let neg42_out1 = squeeze84_out1.neg();
        let concat85_out1 = burn::tensor::Tensor::cat(
            [neg42_out1, squeeze83_out1].into(),
            3,
        );
        let mul207_out1 = concat85_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add103_out1 = mul206_out1.add(mul207_out1);
        let concat86_out1 = burn::tensor::Tensor::cat(
            [add103_out1, slice166_out1].into(),
            3,
        );
        let reshape306_out1 = concat86_out1.reshape([-1, 257, 64]);
        let transpose205_out1 = reshape306_out1.permute([0, 2, 1]);
        let reshape307_out1 = transpose205_out1.reshape([1, 24, 64, 257]);
        let mul208_out1 = concat84_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul209_out1 = reshape307_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul226_out1 = mul208_out1.matmul(mul209_out1);
        let softmax41_out1 = burn::tensor::activation::softmax(matmul226_out1, 3);
        let matmul227_out1 = softmax41_out1.matmul(transpose204_out1);
        let transpose206_out1 = matmul227_out1.permute([0, 2, 1, 3]);
        let reshape308_out1 = transpose206_out1.reshape([1, 257, 1536]);
        let linear148_out1 = self.linear148.forward(reshape308_out1);
        let add104_out1 = add101_out1.add(linear148_out1);
        let layernormalization62_out1 = {
            let dtype = add104_out1.clone().dtype();
            self.layernormalization62
                .forward(add104_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear149_out1 = self.linear149.forward(layernormalization62_out1);
        let reshape309_out1 = linear149_out1.reshape([1, 257, 24, 64]);
        let transpose207_out1 = reshape309_out1.permute([0, 2, 1, 3]);
        let linear150_out1 = self.linear150.forward(linear2_out1.clone());
        let split_tensors = linear150_out1.split(768, 2);
        let [split62_out1, split62_out2] = split_tensors.try_into().unwrap();
        let reshape310_out1 = split62_out1.reshape([1, 130, 12, 64]);
        let transpose208_out1 = reshape310_out1.permute([0, 2, 1, 3]);
        let reshape311_out1 = split62_out2.reshape([1, 130, 12, 64]);
        let transpose209_out1 = reshape311_out1.permute([0, 2, 1, 3]);
        let unsqueeze43_out1: Tensor<B, 5> = transpose208_out1.unsqueeze_dims::<5>(&[2]);
        let expand41_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze43_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze43_out1.expand(shape)
        };
        let unsqueeze44_out1: Tensor<B, 5> = transpose209_out1.unsqueeze_dims::<5>(&[2]);
        let expand42_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze44_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze44_out1.expand(shape)
        };
        let reshape312_out1 = expand42_out1.reshape([1, -1, 130, 64]);
        let reshape313_out1 = expand41_out1.reshape([24, 130, 64]);
        let transpose210_out1 = reshape313_out1.permute([0, 2, 1]);
        let reshape314_out1 = transpose210_out1.reshape([1, 24, 64, 130]);
        let mul210_out1 = transpose207_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul211_out1 = reshape314_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul231_out1 = mul210_out1.matmul(mul211_out1);
        let softmax42_out1 = burn::tensor::activation::softmax(matmul231_out1, 3);
        let matmul232_out1 = softmax42_out1.matmul(reshape312_out1);
        let transpose211_out1 = matmul232_out1.permute([0, 2, 1, 3]);
        let reshape315_out1 = transpose211_out1.reshape([1, 257, 1536]);
        let linear151_out1 = self.linear151.forward(reshape315_out1);
        let add105_out1 = add104_out1.add(linear151_out1);
        let layernormalization63_out1 = {
            let dtype = add105_out1.clone().dtype();
            self.layernormalization63
                .forward(add105_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear152_out1 = self.linear152.forward(layernormalization63_out1);
        let split_tensors = linear152_out1.split(6144, 2);
        let [split63_out1, split63_out2] = split_tensors.try_into().unwrap();
        let sigmoid23_out1 = burn::tensor::activation::sigmoid(split63_out2.clone());
        let mul212_out1 = split63_out2.mul(sigmoid23_out1);
        let mul213_out1 = split63_out1.mul(mul212_out1);
        let linear153_out1 = self.linear153.forward(mul213_out1);
        let add106_out1 = add105_out1.add(linear153_out1);
        let layernormalization64_out1 = {
            let dtype = add106_out1.clone().dtype();
            self.layernormalization64
                .forward(add106_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear154_out1 = self.linear154.forward(layernormalization64_out1);
        let split_tensors = linear154_out1.split(1536, 2);
        let [split64_out1, split64_out2, split64_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape316_out1 = split64_out1.reshape([1, 257, 24, 64]);
        let transpose212_out1 = reshape316_out1.permute([0, 2, 1, 3]);
        let reshape317_out1 = split64_out2.reshape([1, 257, 24, 64]);
        let transpose213_out1 = reshape317_out1.permute([0, 2, 1, 3]);
        let reshape318_out1 = split64_out3.reshape([1, 257, 24, 64]);
        let transpose214_out1 = reshape318_out1.permute([0, 2, 1, 3]);
        let slice169_out1 = transpose212_out1.clone().slice(s![.., .., .., 0..32]);
        let slice170_out1 = transpose212_out1.slice(s![.., .., .., 32..]);
        let mul214_out1 = slice169_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape319_out1 = slice169_out1.reshape([1, 24, 257, 2, 16]);
        let slice171_out1 = reshape319_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze85_out1 = slice171_out1.squeeze_dims::<4>(&[-2]);
        let slice172_out1 = reshape319_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze86_out1 = slice172_out1.squeeze_dims::<4>(&[-2]);
        let neg43_out1 = squeeze86_out1.neg();
        let concat87_out1 = burn::tensor::Tensor::cat(
            [neg43_out1, squeeze85_out1].into(),
            3,
        );
        let mul215_out1 = concat87_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add107_out1 = mul214_out1.add(mul215_out1);
        let concat88_out1 = burn::tensor::Tensor::cat(
            [add107_out1, slice170_out1].into(),
            3,
        );
        let slice173_out1 = transpose213_out1.clone().slice(s![.., .., .., 0..32]);
        let slice174_out1 = transpose213_out1.slice(s![.., .., .., 32..]);
        let mul216_out1 = slice173_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape320_out1 = slice173_out1.reshape([1, 24, 257, 2, 16]);
        let slice175_out1 = reshape320_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze87_out1 = slice175_out1.squeeze_dims::<4>(&[-2]);
        let slice176_out1 = reshape320_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze88_out1 = slice176_out1.squeeze_dims::<4>(&[-2]);
        let neg44_out1 = squeeze88_out1.neg();
        let concat89_out1 = burn::tensor::Tensor::cat(
            [neg44_out1, squeeze87_out1].into(),
            3,
        );
        let mul217_out1 = concat89_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add108_out1 = mul216_out1.add(mul217_out1);
        let concat90_out1 = burn::tensor::Tensor::cat(
            [add108_out1, slice174_out1].into(),
            3,
        );
        let reshape321_out1 = concat90_out1.reshape([-1, 257, 64]);
        let transpose215_out1 = reshape321_out1.permute([0, 2, 1]);
        let reshape322_out1 = transpose215_out1.reshape([1, 24, 64, 257]);
        let mul218_out1 = concat88_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul219_out1 = reshape322_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul237_out1 = mul218_out1.matmul(mul219_out1);
        let softmax43_out1 = burn::tensor::activation::softmax(matmul237_out1, 3);
        let matmul238_out1 = softmax43_out1.matmul(transpose214_out1);
        let transpose216_out1 = matmul238_out1.permute([0, 2, 1, 3]);
        let reshape323_out1 = transpose216_out1.reshape([1, 257, 1536]);
        let linear155_out1 = self.linear155.forward(reshape323_out1);
        let add109_out1 = add106_out1.add(linear155_out1);
        let layernormalization65_out1 = {
            let dtype = add109_out1.clone().dtype();
            self.layernormalization65
                .forward(add109_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear156_out1 = self.linear156.forward(layernormalization65_out1);
        let reshape324_out1 = linear156_out1.reshape([1, 257, 24, 64]);
        let transpose217_out1 = reshape324_out1.permute([0, 2, 1, 3]);
        let linear157_out1 = self.linear157.forward(linear2_out1.clone());
        let split_tensors = linear157_out1.split(768, 2);
        let [split65_out1, split65_out2] = split_tensors.try_into().unwrap();
        let reshape325_out1 = split65_out1.reshape([1, 130, 12, 64]);
        let transpose218_out1 = reshape325_out1.permute([0, 2, 1, 3]);
        let reshape326_out1 = split65_out2.reshape([1, 130, 12, 64]);
        let transpose219_out1 = reshape326_out1.permute([0, 2, 1, 3]);
        let unsqueeze45_out1: Tensor<B, 5> = transpose218_out1.unsqueeze_dims::<5>(&[2]);
        let expand43_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze45_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze45_out1.expand(shape)
        };
        let unsqueeze46_out1: Tensor<B, 5> = transpose219_out1.unsqueeze_dims::<5>(&[2]);
        let expand44_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze46_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze46_out1.expand(shape)
        };
        let reshape327_out1 = expand44_out1.reshape([1, -1, 130, 64]);
        let reshape328_out1 = expand43_out1.reshape([24, 130, 64]);
        let transpose220_out1 = reshape328_out1.permute([0, 2, 1]);
        let reshape329_out1 = transpose220_out1.reshape([1, 24, 64, 130]);
        let mul220_out1 = transpose217_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul221_out1 = reshape329_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul242_out1 = mul220_out1.matmul(mul221_out1);
        let softmax44_out1 = burn::tensor::activation::softmax(matmul242_out1, 3);
        let matmul243_out1 = softmax44_out1.matmul(reshape327_out1);
        let transpose221_out1 = matmul243_out1.permute([0, 2, 1, 3]);
        let reshape330_out1 = transpose221_out1.reshape([1, 257, 1536]);
        let linear158_out1 = self.linear158.forward(reshape330_out1);
        let add110_out1 = add109_out1.add(linear158_out1);
        let layernormalization66_out1 = {
            let dtype = add110_out1.clone().dtype();
            self.layernormalization66
                .forward(add110_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear159_out1 = self.linear159.forward(layernormalization66_out1);
        let split_tensors = linear159_out1.split(6144, 2);
        let [split66_out1, split66_out2] = split_tensors.try_into().unwrap();
        let sigmoid24_out1 = burn::tensor::activation::sigmoid(split66_out2.clone());
        let mul222_out1 = split66_out2.mul(sigmoid24_out1);
        let mul223_out1 = split66_out1.mul(mul222_out1);
        let linear160_out1 = self.linear160.forward(mul223_out1);
        let add111_out1 = add110_out1.add(linear160_out1);
        let layernormalization67_out1 = {
            let dtype = add111_out1.clone().dtype();
            self.layernormalization67
                .forward(add111_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear161_out1 = self.linear161.forward(layernormalization67_out1);
        let split_tensors = linear161_out1.split(1536, 2);
        let [split67_out1, split67_out2, split67_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape331_out1 = split67_out1.reshape([1, 257, 24, 64]);
        let transpose222_out1 = reshape331_out1.permute([0, 2, 1, 3]);
        let reshape332_out1 = split67_out2.reshape([1, 257, 24, 64]);
        let transpose223_out1 = reshape332_out1.permute([0, 2, 1, 3]);
        let reshape333_out1 = split67_out3.reshape([1, 257, 24, 64]);
        let transpose224_out1 = reshape333_out1.permute([0, 2, 1, 3]);
        let slice177_out1 = transpose222_out1.clone().slice(s![.., .., .., 0..32]);
        let slice178_out1 = transpose222_out1.slice(s![.., .., .., 32..]);
        let mul224_out1 = slice177_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape334_out1 = slice177_out1.reshape([1, 24, 257, 2, 16]);
        let slice179_out1 = reshape334_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze89_out1 = slice179_out1.squeeze_dims::<4>(&[-2]);
        let slice180_out1 = reshape334_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze90_out1 = slice180_out1.squeeze_dims::<4>(&[-2]);
        let neg45_out1 = squeeze90_out1.neg();
        let concat91_out1 = burn::tensor::Tensor::cat(
            [neg45_out1, squeeze89_out1].into(),
            3,
        );
        let mul225_out1 = concat91_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add112_out1 = mul224_out1.add(mul225_out1);
        let concat92_out1 = burn::tensor::Tensor::cat(
            [add112_out1, slice178_out1].into(),
            3,
        );
        let slice181_out1 = transpose223_out1.clone().slice(s![.., .., .., 0..32]);
        let slice182_out1 = transpose223_out1.slice(s![.., .., .., 32..]);
        let mul226_out1 = slice181_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape335_out1 = slice181_out1.reshape([1, 24, 257, 2, 16]);
        let slice183_out1 = reshape335_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze91_out1 = slice183_out1.squeeze_dims::<4>(&[-2]);
        let slice184_out1 = reshape335_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze92_out1 = slice184_out1.squeeze_dims::<4>(&[-2]);
        let neg46_out1 = squeeze92_out1.neg();
        let concat93_out1 = burn::tensor::Tensor::cat(
            [neg46_out1, squeeze91_out1].into(),
            3,
        );
        let mul227_out1 = concat93_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add113_out1 = mul226_out1.add(mul227_out1);
        let concat94_out1 = burn::tensor::Tensor::cat(
            [add113_out1, slice182_out1].into(),
            3,
        );
        let reshape336_out1 = concat94_out1.reshape([-1, 257, 64]);
        let transpose225_out1 = reshape336_out1.permute([0, 2, 1]);
        let reshape337_out1 = transpose225_out1.reshape([1, 24, 64, 257]);
        let mul228_out1 = concat92_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul229_out1 = reshape337_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul248_out1 = mul228_out1.matmul(mul229_out1);
        let softmax45_out1 = burn::tensor::activation::softmax(matmul248_out1, 3);
        let matmul249_out1 = softmax45_out1.matmul(transpose224_out1);
        let transpose226_out1 = matmul249_out1.permute([0, 2, 1, 3]);
        let reshape338_out1 = transpose226_out1.reshape([1, 257, 1536]);
        let linear162_out1 = self.linear162.forward(reshape338_out1);
        let add114_out1 = add111_out1.add(linear162_out1);
        let layernormalization68_out1 = {
            let dtype = add114_out1.clone().dtype();
            self.layernormalization68
                .forward(add114_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear163_out1 = self.linear163.forward(layernormalization68_out1);
        let reshape339_out1 = linear163_out1.reshape([1, 257, 24, 64]);
        let transpose227_out1 = reshape339_out1.permute([0, 2, 1, 3]);
        let linear164_out1 = self.linear164.forward(linear2_out1.clone());
        let split_tensors = linear164_out1.split(768, 2);
        let [split68_out1, split68_out2] = split_tensors.try_into().unwrap();
        let reshape340_out1 = split68_out1.reshape([1, 130, 12, 64]);
        let transpose228_out1 = reshape340_out1.permute([0, 2, 1, 3]);
        let reshape341_out1 = split68_out2.reshape([1, 130, 12, 64]);
        let transpose229_out1 = reshape341_out1.permute([0, 2, 1, 3]);
        let unsqueeze47_out1: Tensor<B, 5> = transpose228_out1.unsqueeze_dims::<5>(&[2]);
        let expand45_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze47_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze47_out1.expand(shape)
        };
        let unsqueeze48_out1: Tensor<B, 5> = transpose229_out1.unsqueeze_dims::<5>(&[2]);
        let expand46_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze48_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze48_out1.expand(shape)
        };
        let reshape342_out1 = expand46_out1.reshape([1, -1, 130, 64]);
        let reshape343_out1 = expand45_out1.reshape([24, 130, 64]);
        let transpose230_out1 = reshape343_out1.permute([0, 2, 1]);
        let reshape344_out1 = transpose230_out1.reshape([1, 24, 64, 130]);
        let mul230_out1 = transpose227_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul231_out1 = reshape344_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul253_out1 = mul230_out1.matmul(mul231_out1);
        let softmax46_out1 = burn::tensor::activation::softmax(matmul253_out1, 3);
        let matmul254_out1 = softmax46_out1.matmul(reshape342_out1);
        let transpose231_out1 = matmul254_out1.permute([0, 2, 1, 3]);
        let reshape345_out1 = transpose231_out1.reshape([1, 257, 1536]);
        let linear165_out1 = self.linear165.forward(reshape345_out1);
        let add115_out1 = add114_out1.add(linear165_out1);
        let layernormalization69_out1 = {
            let dtype = add115_out1.clone().dtype();
            self.layernormalization69
                .forward(add115_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear166_out1 = self.linear166.forward(layernormalization69_out1);
        let split_tensors = linear166_out1.split(6144, 2);
        let [split69_out1, split69_out2] = split_tensors.try_into().unwrap();
        let sigmoid25_out1 = burn::tensor::activation::sigmoid(split69_out2.clone());
        let mul232_out1 = split69_out2.mul(sigmoid25_out1);
        let mul233_out1 = split69_out1.mul(mul232_out1);
        let linear167_out1 = self.linear167.forward(mul233_out1);
        let add116_out1 = add115_out1.add(linear167_out1);
        let layernormalization70_out1 = {
            let dtype = add116_out1.clone().dtype();
            self.layernormalization70
                .forward(add116_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear168_out1 = self.linear168.forward(layernormalization70_out1);
        let split_tensors = linear168_out1.split(1536, 2);
        let [split70_out1, split70_out2, split70_out3] = split_tensors
            .try_into()
            .unwrap();
        let reshape346_out1 = split70_out1.reshape([1, 257, 24, 64]);
        let transpose232_out1 = reshape346_out1.permute([0, 2, 1, 3]);
        let reshape347_out1 = split70_out2.reshape([1, 257, 24, 64]);
        let transpose233_out1 = reshape347_out1.permute([0, 2, 1, 3]);
        let reshape348_out1 = split70_out3.reshape([1, 257, 24, 64]);
        let transpose234_out1 = reshape348_out1.permute([0, 2, 1, 3]);
        let slice185_out1 = transpose232_out1.clone().slice(s![.., .., .., 0..32]);
        let slice186_out1 = transpose232_out1.slice(s![.., .., .., 32..]);
        let mul234_out1 = slice185_out1
            .clone()
            .mul(cos2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let reshape349_out1 = slice185_out1.reshape([1, 24, 257, 2, 16]);
        let slice187_out1 = reshape349_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze93_out1 = slice187_out1.squeeze_dims::<4>(&[-2]);
        let slice188_out1 = reshape349_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze94_out1 = slice188_out1.squeeze_dims::<4>(&[-2]);
        let neg47_out1 = squeeze94_out1.neg();
        let concat95_out1 = burn::tensor::Tensor::cat(
            [neg47_out1, squeeze93_out1].into(),
            3,
        );
        let mul235_out1 = concat95_out1
            .mul(sin2_out1.clone().unsqueeze_dims(&[0isize, 1isize]));
        let add117_out1 = mul234_out1.add(mul235_out1);
        let concat96_out1 = burn::tensor::Tensor::cat(
            [add117_out1, slice186_out1].into(),
            3,
        );
        let slice189_out1 = transpose233_out1.clone().slice(s![.., .., .., 0..32]);
        let slice190_out1 = transpose233_out1.slice(s![.., .., .., 32..]);
        let mul236_out1 = slice189_out1
            .clone()
            .mul(cos2_out1.unsqueeze_dims(&[0isize, 1isize]));
        let reshape350_out1 = slice189_out1.reshape([1, 24, 257, 2, 16]);
        let slice191_out1 = reshape350_out1.clone().slice(s![.., .., .., 0..1, ..]);
        let squeeze95_out1 = slice191_out1.squeeze_dims::<4>(&[-2]);
        let slice192_out1 = reshape350_out1.slice(s![.., .., .., 1..2, ..]);
        let squeeze96_out1 = slice192_out1.squeeze_dims::<4>(&[-2]);
        let neg48_out1 = squeeze96_out1.neg();
        let concat97_out1 = burn::tensor::Tensor::cat(
            [neg48_out1, squeeze95_out1].into(),
            3,
        );
        let mul237_out1 = concat97_out1.mul(sin2_out1.unsqueeze_dims(&[0isize, 1isize]));
        let add118_out1 = mul236_out1.add(mul237_out1);
        let concat98_out1 = burn::tensor::Tensor::cat(
            [add118_out1, slice190_out1].into(),
            3,
        );
        let reshape351_out1 = concat98_out1.reshape([-1, 257, 64]);
        let transpose235_out1 = reshape351_out1.permute([0, 2, 1]);
        let reshape352_out1 = transpose235_out1.reshape([1, 24, 64, 257]);
        let mul238_out1 = concat96_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul239_out1 = reshape352_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul259_out1 = mul238_out1.matmul(mul239_out1);
        let softmax47_out1 = burn::tensor::activation::softmax(matmul259_out1, 3);
        let matmul260_out1 = softmax47_out1.matmul(transpose234_out1);
        let transpose236_out1 = matmul260_out1.permute([0, 2, 1, 3]);
        let reshape353_out1 = transpose236_out1.reshape([1, 257, 1536]);
        let linear169_out1 = self.linear169.forward(reshape353_out1);
        let add119_out1 = add116_out1.add(linear169_out1);
        let layernormalization71_out1 = {
            let dtype = add119_out1.clone().dtype();
            self.layernormalization71
                .forward(add119_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear170_out1 = self.linear170.forward(layernormalization71_out1);
        let reshape354_out1 = linear170_out1.reshape([1, 257, 24, 64]);
        let transpose237_out1 = reshape354_out1.permute([0, 2, 1, 3]);
        let linear171_out1 = self.linear171.forward(linear2_out1);
        let split_tensors = linear171_out1.split(768, 2);
        let [split71_out1, split71_out2] = split_tensors.try_into().unwrap();
        let reshape355_out1 = split71_out1.reshape([1, 130, 12, 64]);
        let transpose238_out1 = reshape355_out1.permute([0, 2, 1, 3]);
        let reshape356_out1 = split71_out2.reshape([1, 130, 12, 64]);
        let transpose239_out1 = reshape356_out1.permute([0, 2, 1, 3]);
        let unsqueeze49_out1: Tensor<B, 5> = transpose238_out1.unsqueeze_dims::<5>(&[2]);
        let expand47_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze49_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze49_out1.expand(shape)
        };
        let unsqueeze50_out1: Tensor<B, 5> = transpose239_out1.unsqueeze_dims::<5>(&[2]);
        let expand48_out1 = {
            let onnx_shape: [i64; 5usize] = [1, 1, 2, 1, 1];
            let input_dims = unsqueeze50_out1.dims();
            let mut shape = onnx_shape;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5usize {
                let dim_offset = 5usize - 5usize + i;
                if shape[dim_offset] == 1 && input_dims[i] > 1 {
                    shape[dim_offset] = input_dims[i] as i64;
                }
            }
            unsqueeze50_out1.expand(shape)
        };
        let reshape357_out1 = expand48_out1.reshape([1, -1, 130, 64]);
        let reshape358_out1 = expand47_out1.reshape([24, 130, 64]);
        let transpose240_out1 = reshape358_out1.permute([0, 2, 1]);
        let reshape359_out1 = transpose240_out1.reshape([1, 24, 64, 130]);
        let mul240_out1 = transpose237_out1
            .mul(constant213_out1.clone().unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let mul241_out1 = reshape359_out1
            .mul(constant213_out1.unsqueeze_dims(&[0isize, 1isize, 2isize]));
        let matmul264_out1 = mul240_out1.matmul(mul241_out1);
        let softmax48_out1 = burn::tensor::activation::softmax(matmul264_out1, 3);
        let matmul265_out1 = softmax48_out1.matmul(reshape357_out1);
        let transpose241_out1 = matmul265_out1.permute([0, 2, 1, 3]);
        let reshape360_out1 = transpose241_out1.reshape([1, 257, 1536]);
        let linear172_out1 = self.linear172.forward(reshape360_out1);
        let add120_out1 = add119_out1.add(linear172_out1);
        let layernormalization72_out1 = {
            let dtype = add120_out1.clone().dtype();
            self.layernormalization72
                .forward(add120_out1.clone().cast(burn::tensor::DType::F32))
                .cast(dtype)
        };
        let linear173_out1 = self.linear173.forward(layernormalization72_out1);
        let split_tensors = linear173_out1.split(6144, 2);
        let [split72_out1, split72_out2] = split_tensors.try_into().unwrap();
        let sigmoid26_out1 = burn::tensor::activation::sigmoid(split72_out2.clone());
        let mul242_out1 = split72_out2.mul(sigmoid26_out1);
        let mul243_out1 = split72_out1.mul(mul242_out1);
        let linear174_out1 = self.linear174.forward(mul243_out1);
        let add121_out1 = add120_out1.add(linear174_out1);
        let linear175_out1 = self.linear175.forward(add121_out1);
        let transpose242_out1 = linear175_out1.permute([0, 2, 1]);
        let slice193_out1 = transpose242_out1.slice(s![.., .., 1..]);
        let conv1d2_out1 = self.conv1d2.forward(slice193_out1.clone());
        let add122_out1 = conv1d2_out1.add(slice193_out1);
        add122_out1
    }
}
