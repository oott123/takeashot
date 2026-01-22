#!/usr/bin/env bash
set -eo pipefail
AFS_ID="$1"
BASE_DIR="$(pwd)"

while IFS= read -r line; do
  read -r type_mod type_file path <<< "$line"
  case $type_mod in
    "M")
      case $type_file in
        "f")
          echo "modified file $path"
          if [ -f "${BASE_DIR}${path}" ]; then
            agentfs fs "$AFS_ID" cat "$path" > "${BASE_DIR}${path}"
          else
            echo "File ${BASE_DIR}${path} does not exist!!"
            exit 1
          fi
          ;;
        "d")
          echo "modified directory $path (ignoring)"
          ;;
        *)
          echo "Unknown file type: $type_file"
          ;;
      esac
      ;;
    "A")
      case $type_file in
        "f")
          echo "added file $path"
          if [ ! -f "${BASE_DIR}${path}" ]; then
            agentfs fs "$AFS_ID" cat "$path" > "${BASE_DIR}${path}"
          else
            echo "File ${BASE_DIR}${path} already exists!!"
            exit 1
          fi
          ;;
        "d")
          echo "added directory $path"
          if [ ! -d "${BASE_DIR}${path}" ]; then
            mkdir -p "${BASE_DIR}${path}"
          else
            echo "Directory ${BASE_DIR}${path} already exists!!"
            exit 1
          fi
          ;;
        *)
          echo "Unknown file type: $type_file"
          ;;
      esac
      ;;
    "D")
      case $type_file in
        "f")
          echo "removed file $path"
          if [ -f "${BASE_DIR}${path}" ]; then
            rm "${BASE_DIR}${path}"
          else
            echo "File ${BASE_DIR}${path} does not exist!!"
            exit 1
          fi
          ;;
        "d")
          echo "removed directory $path"
          if [ -d "${BASE_DIR}${path}" ]; then
            rmdir "${BASE_DIR}${path}"
          else
            echo "Directory ${BASE_DIR}${path} does not exist!!"
            exit 1
          fi
          ;;
        *)
          echo "Unknown file type: $type_file"
          ;;
      esac
      ;;
    *)
      echo "Unknown modification type: $type_mod"
      ;;
  esac
done < <(agentfs diff "$AFS_ID" 2>/dev/null)
