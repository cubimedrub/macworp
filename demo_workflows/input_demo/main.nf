#!/usr/bin/env nextflow

nextflow.enable.dsl=2
nextflow.preview.output = true

params.inFile = ""
params.inFolder = ""
params.inFiles = ""
params.inFolders = ""
params.txtFiles = ""
params.numberOfNewlines = ""
params.singleLineText = ""
params.multilineText = ""
params.valueSelect = ""

params.resultsFolder = "results"

process print_params {
    input:
    val inFile
    val inFolder
    val inFiles
    val inFolders
    val txtFiles
    val numberOfNewlines
    val singleLineText
    val multilineText
    val valueSelect

    output:
    path 'params.txt'

    script:
    """
    echo "inFile => $inFile" >> params.txt
    echo "inFiles => $inFiles" >> params.txt
    echo "inFolder => $inFolder" >> params.txt
    echo "txtFiles => $txtFiles" >> params.txt
    echo "inFolders => $inFolders" >> params.txt
    echo "singleLineText => $singleLineText" >> params.txt
    echo "numberOfNewlines => $numberOfNewlines" >> params.txt
    echo "valueSelect => $valueSelect" >> params.txt
    echo "multilineText => $multilineText" >> params.txt

    echo "The following is the full CMD line" >> params.txt

    echo "$workflow.commandLine" >> params.txt
    """
}


workflow  {
    main:
    params_txt = print_params(
        params.inFile,
        params.inFolder,
        params.inFiles,
        params.inFolders,
        params.txtFiles,
        params.numberOfNewlines,
        params.singleLineText,
        params.multilineText,
        params.valueSelect,
    )

    publish:
    params_txt >> params.resultsFolder
}

/**
 * Move the output files to the results folder
 */
output {
    mode "move"
}

/**
 * Once https://github.com/nextflow-io/nextflow/issues/5443#issuecomment-2445609593
 * is resolved and MAcWorP is updated to 24.10
 * we can use the following code to move the output files to the results folder
 * and replace `results >> params.resultsFolder` with `results >> "root"`
 * in the workflow
 */
// output {
//     "root" {
//         mode "move"
//         path "."
//     }
// }