#!/usr/bin/env nextflow

params.inFile = params.inFile ?: { log.error "No inFile file provided."; exit 1 }()
params.inFiles = params.inFiles ?: { log.error "No inFiles files provided."; exit 1 }()
params.inFolder = params.inFolder ?: { log.error "No inFolder file provided."; exit 1 }()
params.inFolders = params.inFolders ?: { log.error "No inFolders files provided."; exit 1 }()
params.numberOfNewlines = params.numberOfNewlines ?: { log.error "No numberOfNewlines provided."; exit 1 }()
params.singleLineText = params.singleLineText ?: { log.error "No singleLineText provided."; exit 1 }()
params.multilineText = params.multilineText ?: { log.error "No multilineText provided."; exit 1 }()
params.txtFiles = params.txtFiles ?: { log.error "No txtFiles provided."; exit 1 }()
params.outDir = "./"

/**
 * Uses the value of NF-Cloud FileGlob to collect txt files with wildcard in path.
 */
text_files = Channel.fromPath(params.txtFiles)

/**
 * Use file from NF-Cloud PathSelector,
 * converts it to uppercase and 
 * returns it.
 */
process readInFileAndConvertToUpperCase {
    input:
    path in_file from params.inFile

    output:
    env in_file_content into in_file_content

    """
    in_file_content=`cat ${in_file} | tr '[a-z]' '[A-Z]'`
    """
}

/**
 * Use file from NF-Cloud PathSelector,
 * converts it to uppercase and 
 * returns it.
 */
process listInFolderContent {
    input:
    path in_folder from params.inFolder

    output:
    env in_folder_content into in_folder_content

    """
    in_folder_content=\$(ls -lah ${in_folder})
    """
}

/**
 * Use comma separated files from NF-Cloud MultiplePathSelector,
 * write them as markdown list to a file
 * and returns file.
 */
process selectedFilesToMarkdownList {
    input:
    val in_files from params.inFiles

    output:
    path selected_files_list into selected_files_list

    """
    for file in \$(echo "${in_files}" | tr ',' ' ')
    do 
        echo "* \$file" >> selected_files_list
    done
    """
}

/**
 * Use comma separated folder from NF-Cloud MultiplePathSelector,
 * write them as markdown list to a file
 * and returns file.
 */
process selectedFoldersToMarkdownList {
    input:
    val in_files from params.inFiles

    output:
    path selected_folder_list into selected_folder_list

    """
    for file in \$(echo "${in_files}" | tr ',' ' ')
    do 
        echo "* \$file" >> selected_folder_list
    done
    """
}

/**
 * Reads the collected txt-files,
 * and write them as markdown code blocks
 * to separate files.
 */
process txtFilesToMarkdownCodeBlocks {
    input:
    path txt_file from text_files

    output:
    path "codeblock_${txt_file.baseName}" into txt_file_codeblocks

    """
    echo "${txt_file}" > codeblock_${txt_file.baseName}
    echo '```' >> codeblock_${txt_file.baseName}
    cat ${txt_file} >> codeblock_${txt_file.baseName}
    echo '```' >> codeblock_${txt_file.baseName}
    """
}

/**
  * Downloads test images for image viewer and SVG viewer
  */  
process downloadTestImages {
    publishDir "${params.outDir}", mode:"copy"

    output:
    path "*.png"
    path "*.svg"

    """
    curl -o ${params.outDir}/test_image.png  https://upload.wikimedia.org/wikipedia/commons/c/c4/PM5544_with_non-PAL_signals.png
    curl -o ${params.outDir}/another_test_image.png  https://upload.wikimedia.org/wikipedia/commons/f/f1/SWTestbild.png
    curl -o ${params.outDir}/test_svg.svg  https://upload.wikimedia.org/wikipedia/commons/0/02/SVG_logo.svg
    """
}

/**
  * Downloads test svg for image viewer
  */  
process downloadTestPDF {
    publishDir "${params.outDir}", mode:"copy"

    output:
    path "*.pdf"

    """
    curl -L -o ${params.outDir}/test_pdf.pdf  https://raw.githubusercontent.com/pdf-association/pdf20examples/master/Simple%20PDF%202.0%20file.pdf
    """
}


/**
 * Put all things together in one markdown file
 * and append NF-Cloud single- & multline TextInput.
 * The resulting outfile will be added to the workdir.
 */
process createMarkdownFile {

    publishDir "${params.outDir}", mode:"copy"

    input:
    val in_file_content from in_file_content
    path selected_files_list from selected_files_list
    val txt_files from txt_file_codeblocks.collect()
    path selected_folder_list from selected_folder_list
    val in_folder_content from in_folder_content

    output:
    path "${params.outDir}/out.md" into out_file

    """
    out_md="${params.outDir}/out.md"

    echo "# Test workflow out.md" >> \$out_md

    echo "## Arguments" >> \$out_md
    echo '* params.inFile: `${params.inFile}`' >> \$out_md
    echo '* params.inFolder: `${params.inFolder}`' >> \$out_md
    echo '* params.inFiles: `${params.inFiles}`' >> \$out_md
    echo '* params.inFolders: `${params.inFolders}`' >> \$out_md
    echo '* params.numberOfNewlines: `${params.numberOfNewlines}`' >> \$out_md
    echo '* params.singleLineText: `${params.singleLineText}`' >> \$out_md
    # sed is for escaping the newlines.
    echo '* params.multilineText: `${params.multilineText.replace("\n", "\\n")}`' >> \$out_md
    echo '* params.txtFiles: `${params.txtFiles}`' >> \$out_md
    echo "" >> \$out_md
    
    echo "## Uppercase content of inFile" >> \$out_md
    echo "${in_file_content}" >> \$out_md
    echo "" >> \$out_md

    echo "## List of inFiles" >> \$out_md
    cat ${selected_files_list} >> \$out_md
    echo "" >> \$out_md

    echo "## inFolder content" >> \$out_md
    echo "${in_folder_content}" >> \$out_md
    echo "" >> \$out_md

    echo "## List of inFolders" >> \$out_md
    cat ${selected_folder_list} >> \$out_md
    echo "" >> \$out_md

    echo "## FileGlob selected file names and content" >> \$out_md
    text_files=(${txt_files.join(' ')})
    for txt_file in \${text_files[@]}
    do
        cat \$txt_file >> \$out_md
    done
    echo "" >> \$out_md

    echo "## Number of newlines following this section" >> \$out_md
    echo "${params.numberOfNewlines}" >> \$out_md
    for i in {1..${params.numberOfNewlines}}
    do 
        echo "" >> \$out_md
    done

    echo "## singleLineText argument" >> \$out_md
    echo "${params.singleLineText}" >> \$out_md
    echo "" >> \$out_md

    echo "## multilineText argument" >> \$out_md
    echo "${params.multilineText}" >> \$out_md
    echo "" >> \$out_md
    """
}