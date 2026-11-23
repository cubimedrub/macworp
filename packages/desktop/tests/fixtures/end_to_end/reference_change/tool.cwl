cwlVersion: v1.2
class: CommandLineTool
# Reference change test tool. CWL has no way to signal the test from inside the tool container,
# so it waits a fixed time (the test changes the reference data meanwhile) and then reads both
# reference files (no command substitution: CWL would parse a nested `$(`); the engine imports `File` inputs, which is what this test finds out about.
requirements:
  DockerRequirement:
    dockerPull: debian:bookworm-slim
  ShellCommandRequirement: {}
inputs:
  ref_a: File
  ref_b: File
arguments:
  - shellQuote: false
    valueFrom: >-
      sleep 60; printf 'a=' > result.txt; cat $(inputs.ref_a.path) >> result.txt; printf ' b=' >> result.txt; cat $(inputs.ref_b.path) >> result.txt
outputs:
  result:
    type: File
    outputBinding:
      glob: result.txt
