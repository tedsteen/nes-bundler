#include "signalsmith-stretch-wrapper.hpp"

std::unique_ptr<SignalsmithStretch> signalsmith_stretch_new() {
  return std::unique_ptr<SignalsmithStretch>(new SignalsmithStretch());
}

void process(InstanceType instance, const SampleFormat *const *inputs, int inputSamples, SampleFormat **outputs, int outputSamples) {
    instance.process(inputs, inputSamples, outputs, outputSamples);
}