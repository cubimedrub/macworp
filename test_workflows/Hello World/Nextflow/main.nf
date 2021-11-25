#!/usr/bin/env nextflow

params.inFile = params.inFile ?: { log.error "No input file provided."; exit 1 }() 
params.outFile = "./result.txt"

process readFile {

    input:
    path in_file from params.inFile

    output:
    env file_content into file_content

    """
    echo ${in_file}
    file_content=`cat ${in_file}`
    """
}

process toUppercase {

    input:
    val file_content from file_content

    output:
    env upper_file_content into upper_file_content

    """
    upper_file_content=`echo '${file_content}' | tr '[a-z]' '[A-Z]'`
    """
}


process writeWordsIntoNewlines {

    input:
    val upper_file_content from upper_file_content

    """
    for word in ${upper_file_content}
    do
        echo \$word
        echo \$word >> "${params.outFile}"
    done
    """
}