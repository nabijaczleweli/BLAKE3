//! Helper functions for efficient IO.

#[cfg(feature = "mmap")]
use std::io::Seek;

#[cfg(feature = "std")]
pub(crate) fn copy_wide(
    mut reader: impl std::io::Read,
    hasher: &mut crate::Hasher,
) -> std::io::Result<u64> {
    let mut buffer = [0; 65536];
    let mut total = 0;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(n) => {
                hasher.update(&buffer[..n]);
                total += n as u64;
            }
            // see test_update_reader_interrupted
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

// Mmap a file, if it looks like a good idea. Return None if we can't or don't want to.
//
// SAFETY: Mmaps are fundamentally unsafe, because you can call invariant-checking functions like
// str::from_utf8 on them and then have them change out from under you. Letting a safe caller get
// their hands on an mmap, or even a &[u8] that's backed by an mmap, is unsound. However, because
// this function is crate-private, we can guarantee that all can ever happen in the event of a race
// condition is that we either hash nonsense bytes or crash with SIGBUS or similar, neither of
// which should risk memory corruption in a safe caller.
//
// PARANOIA: But a data race...is a data race...is a data race...right? Even if we know that no
// platform in the "real world" is ever going to do anything other than compute the "wrong answer"
// if we race on this mmap while we hash it, aren't we still supposed to feel bad about doing this?
// Well, maybe. This is IO, and IO gets special carve-outs in the memory model. Consider a
// memory-mapped register that returns random 32-bit words. (This is actually realistic if you have
// a hardware RNG.) It's probably sound to construct a *const i32 pointing to that register and do
// some raw pointer reads from it. Those reads should be volatile if you don't want the compiler to
// coalesce them, but either way the compiler isn't allowed to just _go nuts_ and insert
// should-never-happen branches to wipe your hard drive if two adjacent reads happen to give
// different values. As far as I'm aware, there's no such thing as a read that's allowed if it's
// volatile but prohibited if it's not (unlike atomics). As mentioned above, it's not ok to
// construct a safe &i32 to the register if you're going to leak that reference to unknown callers.
// But if you "know what you're doing," I don't think *const i32 and &i32 are fundamentally
// different here. Feedback needed.
#[cfg(feature = "mmap")]
pub(crate) fn maybe_mmap_file(file: &mut std::fs::File) -> std::io::Result<Option<memmap2::Mmap>> {
    // Assumes file's seek offset is 0 at entry and is not an observable side-effect if returning Some()
    let file_size = match file.seek(std::io::SeekFrom::End(0)) {
        Ok(l) => l,
        Err(_) => return Ok(None),
    };
    if file_size < 16 * 1024 {
        // Mapping small files is not worth it.
    } else if file_size > usize::MAX as u64 {
        // Too big to map.
    } else if let Ok(map) = unsafe {
        memmap2::MmapOptions::new()
            .len(file_size as usize)
            .map(&*file)
    } {
        return Ok(Some(map));
    }
    file.rewind()?;
    Ok(None)
}
