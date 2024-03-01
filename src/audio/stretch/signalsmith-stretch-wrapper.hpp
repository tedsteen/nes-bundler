#include <memory>
#include "signalsmith-stretch/signalsmith-stretch.h"

typedef float SampleFormat;
typedef signalsmith::stretch::SignalsmithStretch<SampleFormat> SignalsmithStretch;
typedef ::SignalsmithStretch & InstanceType;

std::unique_ptr<SignalsmithStretch> signalsmith_stretch_new();
// Could not find way for cxx to generate the code for the templated method in signalstretch so made this wrapper.
void process(InstanceType instance, const SampleFormat *const *inputs, int inputSamples, SampleFormat **outputs, int outputSamples);
