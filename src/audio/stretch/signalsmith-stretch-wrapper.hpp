#include <memory>
#include "signalsmith-stretch/signalsmith-stretch.h"
typedef float __SampleFormat;
typedef signalsmith::stretch::SignalsmithStretch<__SampleFormat> SignalsmithStretch;
std::unique_ptr<SignalsmithStretch> signalsmith_stretch_new(int nChannels, float sampleRate);
void signalsmith_stretch_process(std::unique_ptr<SignalsmithStretch> &ptr, const __SampleFormat * const * input, int nInputSamples, __SampleFormat **output, int nOutputSamples);