#!/bin/sh
# Download the Creative Commons test media (Internet Archive) into a directory and
# write library.tsv for the server. Needs curl, ffmpeg and ffprobe.
# Usage: scripts/fetch-media.sh [DIR]   (default: ./media-data, which git ignores)
set -eu
DIR="${1:-media-data}"
mkdir -p "$DIR"
cd "$DIR"

get() { # file url
  [ -s "$1" ] || curl -fsSL -o "$1" "$2"
}

get elevatormusic.mp3  "https://archive.org/download/elevatormusic/elevatormusic.mp3"
get breaknorth.ogg     "https://archive.org/download/breaknorth/breaknorth.ogg"
get breaknorth.flac    "https://archive.org/download/breaknorth/breaknorth.flac"
get spinner.mp3        "https://archive.org/download/mt012/01_-_spinner.mp3"
if [ ! -s bbb_soundtrack.m4a ]; then
  # Audio track of the CC-BY film Big Buck Bunny (Blender Foundation), kept as AAC.
  get bbb.mp4 "https://archive.org/download/BigBuckBunny_124/Content/big_buck_bunny_720p_surround.mp4"
  ffmpeg -v error -y -i bbb.mp4 -vn -ac 2 -c:a aac -b:a 160k bbb_soundtrack.m4a
  rm -f bbb.mp4
fi

dur() { ffprobe -v error -show_entries format=duration -of csv=p=0 "$1" | awk '{printf "%d", $1 + 0.5}'; }

# id <TAB> title <TAB> artist <TAB> file <TAB> durationSec <TAB> license <TAB> source
{
  printf 'elevator-music\tElevator Music\tDigi Hartatak\televatormusic.mp3\t%s\tCC BY 3.0\thttps://archive.org/details/elevatormusic\n' "$(dur elevatormusic.mp3)"
  printf 'break-north-ogg\tBreak North (Ogg Vorbis)\tDeadman & Digi Hartatak\tbreaknorth.ogg\t%s\tCC BY 3.0\thttps://archive.org/details/breaknorth\n' "$(dur breaknorth.ogg)"
  printf 'break-north-flac\tBreak North (FLAC)\tDeadman & Digi Hartatak\tbreaknorth.flac\t%s\tCC BY 3.0\thttps://archive.org/details/breaknorth\n' "$(dur breaknorth.flac)"
  printf 'spinner\tSpinner\tSoftspace\tspinner.mp3\t%s\tCC BY 3.0\thttps://archive.org/details/mt012\n' "$(dur spinner.mp3)"
  printf 'bbb-soundtrack\tBig Buck Bunny (film audio)\tBlender Foundation\tbbb_soundtrack.m4a\t%s\tCC BY 3.0\thttps://archive.org/details/BigBuckBunny_124\n' "$(dur bbb_soundtrack.m4a)"
} > library.tsv
echo "wrote $(pwd)/library.tsv"
