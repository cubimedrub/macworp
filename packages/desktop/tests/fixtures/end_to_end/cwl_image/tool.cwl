cwlVersion: v1.2
class: CommandLineTool
# End-to-end test tool of the image build: runs in an image that only exists because the worker
# built it from the Dockerfile next to this file (a pull of the name would fail).
requirements:
  DockerRequirement:
    dockerPull: macworp-e2e/cwl-built:1
  ShellCommandRequirement: {}
inputs:
  message: string
arguments:
  - shellQuote: false
    valueFrom: >-
      printf '%s\n' "$(inputs.message)" > cwl_built.txt; cat /marker.txt >> cwl_built.txt
outputs:
  result:
    type: File
    outputBinding:
      glob: cwl_built.txt
