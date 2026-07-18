#!/usr/bin/env python3
import os
import re
import sys
import shutil
import requests
import tomllib
from pathlib import Path

# --- Configuration ---
def get_api_key():
    # 1. Try environment variable
    key = os.getenv("TMDB_API_KEY")
    if key:
        return key

    # 2. Try trst config file
    config_path = Path.home() / ".config" / "trst" / "config.toml"
    if config_path.exists():
        try:
            with open(config_path, "rb") as f:
                data = tomllib.load(f)
                # Look for [media] tmdb_api_key
                return data.get("media", {}).get("tmdb_api_key")
        except Exception:
            pass

    return None

TMDB_API_KEY = get_api_key()

# Target base directory for organized media.
BASE_MEDIA_DIR = Path("/mnt/hetzner/media")
MOVIES_DIR = BASE_MEDIA_DIR / "movies"
SERIES_DIR = BASE_MEDIA_DIR / "series"

# --- Transmission Environment Variables ---
# These are automatically set by Transmission when running the script.
TORRENT_DIR = os.getenv("TR_TORRENT_DIR")
TORRENT_NAME = os.getenv("TR_TORRENT_NAME")

def log(msg):
    print(f"[Organizer] {msg}")

def parse_name(name):
    """Parses a string (torrent name or filename) to extract title, year, and SxxExx."""
    # Series pattern: S01E01, 1x01, etc.
    series_match = re.search(r'(.*?)[. ]?(?:S(\d{1,2})E(\d{1,2})|(\d{1,2})x(\d{1,2}))', name, re.IGNORECASE)
    if series_match:
        title = series_match.group(1).replace('.', ' ').strip()
        s = series_match.group(2) or series_match.group(4)
        e = series_match.group(3) or series_match.group(5)
        # Check for year inside the title part
        year_match = re.search(r'\(?(\d{4})\)?', title)
        year = year_match.group(1) if year_match else None
        if year:
            title = title.replace(f"({year})", "").replace(year, "").strip()
        return {'type': 'series', 'title': title, 'year': year, 'season': int(s), 'episode': int(e)}

    # Movie pattern: Title (Year) or Title.Year
    movie_match = re.search(r'(.*?)[. ]?\(?(\d{4})\)?', name)
    if movie_match:
        title = movie_match.group(1).replace('.', ' ').strip()
        return {'type': 'movie', 'title': title, 'year': movie_match.group(2)}

    # Fallback: Just clean the name
    return {'type': 'unknown', 'title': name.replace('.', ' ').strip(), 'year': None}

def tmdb_lookup(media_type, title, year=None):
    """Queries TMDB for the canonical title, year, and ID."""
    endpoint = "movie" if media_type == "movie" else "tv"
    params = {
        "api_key": TMDB_API_KEY,
        "query": title,
        "language": "en-US"
    }
    if year:
        params["primary_release_year" if media_type == "movie" else "first_air_date_year"] = year

    try:
        r = requests.get(f"https://api.themoviedb.org/3/search/{endpoint}", params=params, timeout=10)
        r.raise_for_status()
        results = r.json().get("results", [])
        if not results:
            return None

        # Best match selection: prefer exact year if multiple results exist
        best = results[0]
        if year:
            for res in results:
                res_year = (res.get("release_date") or res.get("first_air_date") or "")[:4]
                if res_year == year:
                    best = res
                    break

        return {
            "title": best.get("title") or best.get("name"),
            "year": (best.get("release_date") or best.get("first_air_date") or "0000")[:4],
            "id": best.get("id")
        }
    except Exception as e:
        log(f"TMDB Error: {e}")
        return None

def get_largest_video(path):
    """Finds the largest video file in a directory or returns the file if it is a video."""
    exts = {".mkv", ".mp4", ".avi", ".m4v", ".mov"}
    if path.is_file():
        return path if path.suffix.lower() in exts else None
    files = [f for f in path.rglob("*") if f.suffix.lower() in exts]
    return max(files, key=lambda f: f.stat().st_size) if files else None

def main():
    if not (TORRENT_DIR and TORRENT_NAME):
        log("Environment variables TR_TORRENT_DIR and TR_TORRENT_NAME not found. Run this from Transmission.")
        return

    source_path = Path(TORRENT_DIR) / TORRENT_NAME
    if not source_path.exists():
        log(f"Source not found: {source_path}")
        return

    log(f"Processing: {TORRENT_NAME}")

    if not TMDB_API_KEY:
        log("Error: TMDB_API_KEY not found. Set it in ~/.config/trst/config.toml (tmdb_api_key = \"...\") or as an environment variable.")
        return

    info = parse_name(TORRENT_NAME)

    # If the torrent name didn't provide enough info, inspect the files
    if info['type'] == 'unknown' or not info['year']:
        largest = get_largest_video(source_path)
        if largest:
            f_info = parse_name(largest.name)
            if info['type'] == 'unknown': info['type'] = f_info['type']
            if not info['year']: info['year'] = f_info['year']
            if info['type'] == 'series':
                info['season'], info['episode'] = f_info['season'], f_info['episode']

    # Final fallback if still unknown (treat as movie)
    if info['type'] == 'unknown':
        info['type'] = 'movie'

    # Fetch canonical metadata
    meta = tmdb_lookup(info['type'], info['title'], info['year'])
    if not meta:
        log("Metadata not found on TMDB. Using parsed info.")
        meta = {"title": info['title'], "year": info['year'] or "0000", "id": "unknown"}

    log(f"Canonical Info: {meta['title']} ({meta['year']}) [ID: {meta['id']}]")

    # Define target directories
    base_dir = MOVIES_DIR if info['type'] == 'movie' else SERIES_DIR
    canon_folder_name = f"{meta['title']} ({meta['year']}) [tmdbid-{meta['id']}]"
    target_dir = base_dir / canon_folder_name

    # Strict Normalization: Rename existing directory if it's not in the correct format
    for existing in base_dir.glob("*"):
        if existing.is_dir() and meta['title'].lower() in existing.name.lower() and meta['year'] in existing.name:
            if existing.name != canon_folder_name:
                log(f"Normalizing existing folder: {existing.name} -> {canon_folder_name}")
                try:
                    shutil.move(str(existing), str(target_dir))
                except Exception as e:
                    log(f"Failed to normalize folder: {e}")
            break

    target_dir.mkdir(parents=True, exist_ok=True)

    # Determine final destination and filename
    if info['type'] == 'series':
        final_dest_dir = target_dir / f"Season {info['season']:02d}"
        final_dest_dir.mkdir(exist_ok=True)
        # Series format: Title SXXEYY.ext
        final_filename = f"{meta['title']} S{info['season']:02d}E{info['episode']:02d}"
    else:
        final_dest_dir = target_dir
        # Movie format: Title (Year).ext
        final_filename = f"{meta['title']} ({meta['year']})"

    video = get_largest_video(source_path)
    if not video:
        log("No video files found in torrent.")
        return

    dest_path = final_dest_dir / f"{final_filename}{video.suffix}"

    # Handle filename collisions (avoid overwriting)
    if dest_path.exists():
        log(f"File {dest_path.name} already exists. Appending '- new' suffix.")
        dest_path = final_dest_dir / f"{final_filename} - new{video.suffix}"

    log(f"Moving {video.name} to {dest_path.parent.name}/{dest_path.name}")
    shutil.move(str(video), str(dest_path))

    # Optional: Move subtitles if they match the video or are in a 'Subs' folder
    for srt in source_path.rglob("*.srt") if source_path.is_dir() else []:
        if video.stem in srt.name or srt.parent.name.lower() == "subs":
            lang_match = re.search(r'\.([a-z]{2,3})\.srt$', srt.name, re.IGNORECASE)
            lang_ext = f".{lang_match.group(1)}.srt" if lang_match else ".srt"
            dest_srt = final_dest_dir / f"{final_filename}{lang_ext}"
            try:
                shutil.move(str(srt), str(dest_srt))
            except:
                pass

    # Cleanup: Remove unneeded files and original source
    if source_path.exists():
        if source_path.is_dir():
            log(f"Cleaning up source directory: {source_path}")
            shutil.rmtree(source_path)
        else:
            log(f"Removing source file: {source_path}")
            source_path.unlink()

    log("Successfully organized.")

if __name__ == "__main__":
    main()
