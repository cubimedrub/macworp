cwlVersion: v1.2
class: CommandLineTool
# End-to-end test tool: writes its text, number and the content of its input file into
# cwl_result.txt, which the CWL engine collects into the project directory. Runs in an image,
# which exercises the sibling containers of the engine.
requirements:
  DockerRequirement:
    dockerPull: debian:bookworm-slim
  ShellCommandRequirement: {}
inputs:
  message: string
  count: int
  input: File
arguments:
  - shellQuote: false
    valueFrom: >-
      printf '%s\n%s\n' "$(inputs.message)" "$(inputs.count)" > cwl_result.txt; cat $(inputs.input.path) >> cwl_result.txt
outputs:
  result:
    type: File
    outputBinding:
      glob: cwl_result.txt
