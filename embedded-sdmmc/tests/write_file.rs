//! File opening related tests

use embedded_sdmmc::{Mode, VolumeIdx, VolumeManager};

mod utils;

#[test]
fn append_file() {
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(0))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");

    // Open with string
    let f = volume_mgr
        .open_file_in_dir(root_dir, "README.TXT", Mode::ReadWriteTruncate)
        .expect("open file");

    // Should be enough to cause a few more clusters to be allocated
    let test_data = vec![0xCC; 1024 * 1024];
    volume_mgr.write(f, &test_data).expect("file write");

    let length = volume_mgr.file_length(f).expect("get length");
    assert_eq!(length, 1024 * 1024);

    let offset = volume_mgr.file_offset(f).expect("offset");
    assert_eq!(offset, 1024 * 1024);

    // Now wind it back 1 byte;
    volume_mgr.file_seek_from_current(f, -1).expect("Seeking");

    let offset = volume_mgr.file_offset(f).expect("offset");
    assert_eq!(offset, (1024 * 1024) - 1);

    // Write another megabyte, making `2 MiB - 1`
    volume_mgr.write(f, &test_data).expect("file write");

    let length = volume_mgr.file_length(f).expect("get length");
    assert_eq!(length, (1024 * 1024 * 2) - 1);

    volume_mgr.close_file(f).expect("close dir");

    // Now check the file length again

    let entry = volume_mgr
        .find_directory_entry(root_dir, "README.TXT")
        .expect("Find entry");
    assert_eq!(entry.size, (1024 * 1024 * 2) - 1);

    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");
}

#[test]
fn flush_file() {
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(0))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");

    // Open with string
    let f = volume_mgr
        .open_file_in_dir(root_dir, "README.TXT", Mode::ReadWriteTruncate)
        .expect("open file");

    // Write some data to the file
    let test_data = vec![0xCC; 64];
    volume_mgr.write(f, &test_data).expect("file write");

    // Check that the file length is zero in the directory entry, as we haven't
    // flushed yet
    let entry = volume_mgr
        .find_directory_entry(root_dir, "README.TXT")
        .expect("find entry");
    assert_eq!(entry.size, 0);

    volume_mgr.flush_file(f).expect("flush");

    // Now check the file length again after flushing
    let entry = volume_mgr
        .find_directory_entry(root_dir, "README.TXT")
        .expect("find entry");
    assert_eq!(entry.size, 64);

    // Flush more writes
    volume_mgr.write(f, &test_data).expect("file write");
    volume_mgr.write(f, &test_data).expect("file write");
    volume_mgr.flush_file(f).expect("flush");

    // Now check the file length again, again
    let entry = volume_mgr
        .find_directory_entry(root_dir, "README.TXT")
        .expect("find entry");
    assert_eq!(entry.size, 64 * 3);
}

#[test]
fn random_access_write_file() {
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(0))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");

    // Open with string
    let f = volume_mgr
        .open_file_in_dir(root_dir, "README.TXT", Mode::ReadWriteTruncate)
        .expect("open file");

    let test_data = vec![0xCC; 1024];
    volume_mgr.write(f, &test_data).expect("file write");

    let length = volume_mgr.file_length(f).expect("get length");
    assert_eq!(length, 1024);

    for seek_offset in [100, 0] {
        let mut expected_buffer = [0u8; 4];

        // fetch some data at offset seek_offset
        volume_mgr
            .file_seek_from_start(f, seek_offset)
            .expect("Seeking");
        volume_mgr.read(f, &mut expected_buffer).expect("read file");

        // modify first byte
        expected_buffer[0] ^= 0xff;

        // write only first byte, expecting the rest to not change
        volume_mgr
            .file_seek_from_start(f, seek_offset)
            .expect("Seeking");
        volume_mgr
            .write(f, &expected_buffer[0..1])
            .expect("file write");
        volume_mgr.flush_file(f).expect("file flush");

        // read and verify
        volume_mgr
            .file_seek_from_start(f, seek_offset)
            .expect("file seek");
        let mut read_buffer = [0xffu8, 0xff, 0xff, 0xff];
        volume_mgr.read(f, &mut read_buffer).expect("file read");
        assert_eq!(
            read_buffer, expected_buffer,
            "mismatch seek+write at offset {seek_offset} from start"
        );
    }

    volume_mgr.close_file(f).expect("close file");
    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");
}
// ****************************************************************************
//
// End Of File
//
// ****************************************************************************

/// The FAT32 partition starts at block 264192; its FSInfo sector is the next
/// one, with the free cluster count at byte 488.
const FAT32_FSINFO: embedded_sdmmc::BlockIdx = embedded_sdmmc::BlockIdx(264192 + 1);

fn free_count(disk: &utils::RamDisk<Vec<u8>>) -> u32 {
    use embedded_sdmmc::{Block, BlockDevice};
    let mut block = [Block::new()];
    disk.read(&mut block, FAT32_FSINFO).unwrap();
    u32::from_le_bytes(block[0].contents[488..492].try_into().unwrap())
}

/// Write and delete a 1 MiB file more times than the free space would hold:
/// fails with a full disk if delete leaves the file's clusters allocated.
/// Returns the disk once the volume is closed.
fn delete_frees_clusters(
    volume_idx: VolumeIdx,
    file_mib: usize,
    rounds: usize,
) -> utils::RamDisk<Vec<u8>> {
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr.open_raw_volume(volume_idx).expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");
    let test_data = vec![0xCC; file_mib * 1024 * 1024];
    for round in 0..rounds {
        let f = volume_mgr
            .open_file_in_dir(root_dir, "LEAK.DAT", Mode::ReadWriteCreateOrTruncate)
            .expect("open file");
        volume_mgr
            .write(f, &test_data)
            .unwrap_or_else(|e| panic!("write in round {round}: {e:?}"));
        volume_mgr.close_file(f).expect("close file");
        volume_mgr
            .delete_entry_in_dir(root_dir, "LEAK.DAT")
            .expect("delete file");
    }
    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");
    volume_mgr.free().0
}

#[test]
fn delete_frees_clusters_fat16() {
    // About 60 MiB free on the FAT16 partition.
    delete_frees_clusters(VolumeIdx(0), 1, 80);
}

#[test]
fn delete_frees_clusters_fat32() {
    let before = free_count(&utils::make_block_device(utils::DISK_SOURCE).unwrap());
    // About 320 MiB free on the FAT32 partition.
    let disk = delete_frees_clusters(VolumeIdx(1), 8, 50);
    // Every cluster allocated was freed and counted again
    assert_eq!(free_count(&disk), before);
}

#[test]
fn truncate_counts_every_freed_cluster() {
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let before = free_count(&disk);
    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(1))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");
    let f = volume_mgr
        .open_file_in_dir(root_dir, "TRUNC.DAT", Mode::ReadWriteCreateOrTruncate)
        .expect("open file");
    volume_mgr
        .write(f, &vec![0xCC; 1024 * 1024])
        .expect("file write");
    volume_mgr.close_file(f).expect("close file");
    let f = volume_mgr
        .open_file_in_dir(root_dir, "TRUNC.DAT", Mode::ReadWriteTruncate)
        .expect("open file");
    volume_mgr.close_file(f).expect("close file");
    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");
    // A truncated file keeps its first cluster
    assert_eq!(free_count(&volume_mgr.free().0), before - 1);
}

#[test]
fn delete_stops_at_a_link_outside_the_volume() {
    use embedded_sdmmc::{Block, BlockDevice, BlockIdx};
    // The FAT16 partition starts at block 2048.
    const PARTITION: u32 = 2048;
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(0))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");
    let f = volume_mgr
        .open_file_in_dir(root_dir, "BROKEN.DAT", Mode::ReadWriteCreateOrTruncate)
        .expect("open file");
    volume_mgr
        .write(f, &vec![0xCC; 64 * 1024])
        .expect("file write");
    volume_mgr.close_file(f).expect("close file");
    let entry = volume_mgr
        .find_directory_entry(root_dir, "BROKEN.DAT")
        .expect("find entry");
    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");
    let (disk, time_source) = volume_mgr.free();

    // The file's first cluster, from its directory entry
    let mut block = [Block::new()];
    disk.read(&mut block, entry.entry_block).unwrap();
    let e = &block[0].contents[entry.entry_offset as usize..];
    let first = u32::from(u16::from_le_bytes([e[26], e[27]]));
    // Point that cluster's FAT16 entry at cluster 0, as a chain crossing a
    // cluster another delete freed would
    let mut boot = [Block::new()];
    disk.read(&mut boot, BlockIdx(PARTITION)).unwrap();
    let reserved = u32::from(u16::from_le_bytes([
        boot[0].contents[14],
        boot[0].contents[15],
    ]));
    let fat_start = PARTITION + reserved;
    let fat_block = BlockIdx(fat_start + first * 2 / 512);
    disk.read(&mut block, fat_block).unwrap();
    let at = (first * 2 % 512) as usize;
    block[0].contents[at..at + 2].copy_from_slice(&0u16.to_le_bytes());
    disk.write(&block, fat_block).unwrap();
    let mut fat0_before = [Block::new()];
    disk.read(&mut fat0_before, BlockIdx(fat_start)).unwrap();

    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(0))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");
    assert!(matches!(
        volume_mgr.delete_entry_in_dir(root_dir, "BROKEN.DAT"),
        Err(embedded_sdmmc::Error::FormatError(_))
    ));
    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");
    // FAT[0] and FAT[1] (the media descriptor) are untouched
    let mut fat0_after = [Block::new()];
    volume_mgr
        .free()
        .0
        .read(&mut fat0_after, BlockIdx(fat_start))
        .unwrap();
    assert_eq!(fat0_after[0].contents[0..4], fat0_before[0].contents[0..4]);
}

/// FSInfo's free cluster count is only a hint. One that is too low must not
/// underflow as clusters are allocated: the count becomes unknown, and is
/// saved as unknown (0xFFFF_FFFF).
#[test]
fn free_count_too_low_becomes_unknown() {
    use embedded_sdmmc::{Block, BlockDevice, BlockIdx};
    const FSINFO: BlockIdx = FAT32_FSINFO;
    let time_source = utils::make_time_source();
    let disk = utils::make_block_device(utils::DISK_SOURCE).unwrap();
    let mut block = [Block::new()];
    disk.read(&mut block, FSINFO).unwrap();
    block[0].contents[488..492].copy_from_slice(&1u32.to_le_bytes());
    disk.write(&block, FSINFO).unwrap();

    let volume_mgr: VolumeManager<utils::RamDisk<Vec<u8>>, utils::TestTimeSource, 4, 2, 1> =
        VolumeManager::new_with_limits(disk, time_source, 0xAA00_0000);
    let volume = volume_mgr
        .open_raw_volume(VolumeIdx(1))
        .expect("open volume");
    let root_dir = volume_mgr.open_root_dir(volume).expect("open root dir");
    let f = volume_mgr
        .open_file_in_dir(root_dir, "LOW.DAT", Mode::ReadWriteCreateOrTruncate)
        .expect("open file");
    volume_mgr
        .write(f, &vec![0xCC; 1024 * 1024])
        .expect("file write");
    volume_mgr.close_file(f).expect("close file");
    volume_mgr.close_dir(root_dir).expect("close dir");
    volume_mgr.close_volume(volume).expect("close volume");

    let (disk, _time_source) = volume_mgr.free();
    disk.read(&mut block, FSINFO).unwrap();
    assert_eq!(block[0].contents[488..492], 0xFFFF_FFFFu32.to_le_bytes());
}
