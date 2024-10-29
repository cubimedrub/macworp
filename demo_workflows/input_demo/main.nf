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

params.resultsFolder = 'results'

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
    params_txt >> "default"
}

/**
 * Move the output files to the results folder
 */
output {
    "default" {
        mode 'move'
        path params.resultsFolder
    }
}