macro_rules! use_display_model {
    ($model:ident) => {
        pub type PixelColor = <$model as mipidsi::models::Model>::ColorFormat;
        #[repr(transparent)]
        pub struct DisplayModel($model);
        impl mipidsi::models::Model for DisplayModel {
            type ColorFormat = <$model as mipidsi::models::Model>::ColorFormat;

            const FRAMEBUFFER_SIZE: (u16, u16) =
                <$model as mipidsi::models::Model>::FRAMEBUFFER_SIZE;

            #[inline(always)]
            fn init<RST, DELAY, DI>(
                &mut self,
                dcs: &mut mipidsi::dcs::Dcs<DI>,
                delay: &mut DELAY,
                options: &mipidsi::options::ModelOptions,
                rst: &mut Option<RST>,
            ) -> Result<mipidsi::dcs::SetAddressMode, mipidsi::error::InitError<RST::Error>>
            where
                RST: OutputPin,
                DELAY: DelayNs,
                DI: WriteOnlyDataCommand,
            {
                <$model as mipidsi::models::Model>::init(&mut self.0, dcs, delay, options, rst)
            }

            #[inline(always)]
            fn write_pixels<DI, I>(
                &mut self,
                di: &mut mipidsi::dcs::Dcs<DI>,
                colors: I,
            ) -> Result<(), mipidsi::error::Error>
            where
                DI: WriteOnlyDataCommand,
                I: IntoIterator<Item = Self::ColorFormat>,
            {
                <$model as mipidsi::models::Model>::write_pixels(&mut self.0, di, colors)
            }
        }
        impl Default for DisplayModel {
            fn default() -> Self {
                DisplayModel($model)
            }
        }
    };
}
