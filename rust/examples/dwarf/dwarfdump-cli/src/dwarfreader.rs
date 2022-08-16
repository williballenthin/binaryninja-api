use gimli::{Endianity, Error, Reader, ReaderOffsetId};

use std::borrow::Cow;
use std::convert::TryInto;
use std::{fmt, str};

#[derive(Clone)]
pub(crate) struct DWARFReader<Endian: Endianity> {
  data: Vec<u8>,
  endian: Endian,
  data_offset: usize,
  section_offset: usize,
}

impl<Endian: Endianity> DWARFReader<Endian> {
  pub fn new(data: Vec<u8>, endian: Endian) -> Self {
    Self {
      data,
      endian,
      data_offset: 0,
      section_offset: 0,
    }
  }
}

impl<Endian: Endianity> fmt::Debug for DWARFReader<Endian> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let data = if self.data.len() < 6 {
      self.data.clone()
    } else {
      let mut vec = vec![0; 6];
      vec.clone_from_slice(&self.data[0..6]);
      vec
    };
    f.debug_struct("DWARFReader")
      .field("data", &data)
      .field("endian", &self.endian)
      .field("data_offset", &self.data_offset)
      .field("section_offset", &self.section_offset)
      .finish()
  }
}

impl<Endian: Endianity> Reader for DWARFReader<Endian> {
  type Endian = Endian;
  type Offset = usize;

  fn endian(&self) -> Endian {
    println!("endian ({:?})", self.endian);
    self.endian
  }

  fn len(&self) -> usize {
    println!("len ({:?})", self.data.len() - self.data_offset);
    self.data.len() - self.data_offset
  }

  fn empty(&mut self) {
    println!("empty");
    self.data.clear();
    self.data_offset = 0;
  }

  fn truncate(&mut self, len: usize) -> Result<(), Error> {
    println!("truncate");
    self.data.truncate(self.data_offset + len);
    Ok(())
  }

  fn offset_from(&self, base: &Self) -> usize {
    println!("offset_from");
    (self.section_offset + self.data_offset) - (base.section_offset + base.data_offset)
  }

  fn offset_id(&self) -> ReaderOffsetId {
    println!("offset_id");
    ReaderOffsetId(self.data_offset.try_into().unwrap())
  }

  fn lookup_offset_id(&self, id: ReaderOffsetId) -> Option<usize> {
    println!("lookup_offset_id");
    Some(id.0.try_into().unwrap())
  }

  fn find(&self, byte: u8) -> Result<usize, Error> {
    println!("find");
    match self
      .data
      .iter()
      .skip(self.data_offset)
      .position(|&b| b == byte)
    {
      Some(value) => Ok(value),
      _ => Err(Error::UnexpectedEof(self.offset_id())),
    }
  }

  fn skip(&mut self, len: usize) -> Result<(), Error> {
    // println!(
    //   "skip ({:?}, {:?}->{:?})",
    //   len,
    //   self.data_offset,
    //   self.data_offset + len
    // );
    println!("skip ({:?})", len,);

    if self.data.len() < self.data_offset + len {
      Err(Error::UnexpectedEof(self.offset_id()))
    } else {
      self.data_offset += len;
      Ok(())
    }
  }

  fn split(&mut self, len: usize) -> Result<Self, Error> {
    println!("split");
    // println!("  Current data length   : {:?}", self.data.len());
    println!("  Current reader length : {:?}", self.len());
    println!(
      "  Current reader data_offset : {:?}",
      self.section_offset + self.data_offset
    );
    println!("  Requested split size  : {:?}", len);

    if self.data.len() < self.data_offset + len {
      println!("  ERROR!");
      assert!(false);
      Err(Error::UnexpectedEof(self.offset_id()))
    } else {
      self.data_offset += len;
      // println!("  New reader data_offset     : {:?}", self.data_offset);

      Ok(Self {
        data: self.data[(self.data_offset - len)..self.data_offset]
          .into_iter()
          .map(|b| b.clone())
          .collect(),
        endian: self.endian,
        data_offset: 0,
        section_offset: self.section_offset + self.data_offset - len,
      })
    }
  }

  fn to_slice(&self) -> Result<Cow<'_, [u8]>, Error> {
    println!("to_slice");
    // println!("  Current data length   : {:?}", self.data.len());
    println!("  Current reader length : {:?}", self.len());
    println!(
      "  Current reader data_offset : {:?}",
      self.section_offset + self.data_offset
    );
    Ok(self.data[self.data_offset..].into())
  }

  fn to_string(&self) -> Result<Cow<'_, str>, Error> {
    println!("to_string");
    Ok(
      str::from_utf8(&self.data[self.data_offset..])
        .unwrap()
        .into(),
    )
  }

  fn to_string_lossy(&self) -> Result<Cow<'_, str>, Error> {
    println!("to_string_lossy");
    Ok(
      str::from_utf8(&self.data[self.data_offset..])
        .unwrap()
        .into(),
    )
  }

  fn read_slice(&mut self, buf: &mut [u8]) -> Result<(), Error> {
    println!("read_slice");
    // println!("  Current data length   : {:?}", self.data.len());
    println!("  Current reader length : {:?}", self.len());
    println!(
      "  Current reader data_offset : {:?}",
      self.section_offset + self.data_offset
    );
    println!("  Requested buffer len  : {:?}", buf.len());

    if self.len() >= 4 {
      let mut vec = vec![0; 4];
      vec.clone_from_slice(&self.data[self.data_offset..self.data_offset + 4]);
      println!("  data: {:?}", vec);
    }

    if self.data.len() < self.data_offset + buf.len() {
      println!("  ERROR!");
      Err(Error::UnexpectedEof(self.offset_id()))
    } else {
      for b in buf {
        *b = self.data[self.data_offset];
        self.data_offset += 1;
      }

      Ok(())
    }
  }

  //////////////////////////////////

  /// These are all here only to mirror the printing behavior of the reference dwarf-dump....they're safe to delete for final implementation, since they'll fall back on the trait's default implementations

  // /// Read a u8.
  // #[inline]
  // fn read_u8(&mut self) -> Result<u8, Error> {
  //   if self.data.len() - self.data_offset > 0 {
  //     self.data_offset += 1;
  //     Ok(self.data[self.data_offset - 1])
  //   } else {
  //     Err(Error::UnexpectedEof(self.offset_id()))
  //   }
  // }

  // /// Read a u16.
  // #[inline]
  // fn read_u16(&mut self) -> Result<u16, Error> {
  //   if self.data.len() - self.data_offset > 1 {
  //     self.data_offset += 2;
  //     Ok(
  //       self.endian.read_u16(
  //         self.data[self.data_offset - 2..self.data_offset]
  //           .try_into()
  //           .unwrap(),
  //       ),
  //     )
  //   } else {
  //     Err(Error::UnexpectedEof(self.offset_id()))
  //   }
  // }

  // /// Read a u32.
  // #[inline]
  // fn read_u32(&mut self) -> Result<u32, Error> {
  //   if self.data.len() - self.data_offset > 3 {
  //     self.data_offset += 4;
  //     Ok(
  //       self.endian.read_u32(
  //         self.data[self.data_offset - 4..self.data_offset]
  //           .try_into()
  //           .unwrap(),
  //       ),
  //     )
  //   } else {
  //     Err(Error::UnexpectedEof(self.offset_id()))
  //   }
  // }

  // /// Read a u64.
  // #[inline]
  // fn read_u64(&mut self) -> Result<u64, Error> {
  //   if self.data.len() - self.data_offset > 7 {
  //     self.data_offset += 8;
  //     Ok(
  //       self.endian.read_u64(
  //         self.data[self.data_offset - 8..self.data_offset]
  //           .try_into()
  //           .unwrap(),
  //       ),
  //     )
  //   } else {
  //     Err(Error::UnexpectedEof(self.offset_id()))
  //   }
  // }

  fn read_offset(&mut self, format: gimli::Format) -> gimli::Result<usize> {
    println!("read_offset");

    match format {
      gimli::Format::Dwarf32 => match {
        if self.data.len() - self.data_offset > 3 {
          self.data_offset += 4;
          Ok(
            self.endian.read_u32(
              self.data[self.data_offset - 4..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      } {
        Ok(value) => Ok(<usize as gimli::ReaderOffset>::from_u32(value)),
        Err(e) => Err(e),
      },
      gimli::Format::Dwarf64 => match {
        if self.data.len() - self.data_offset > 7 {
          self.data_offset += 8;
          Ok(
            self.endian.read_u64(
              self.data[self.data_offset - 8..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      } {
        Ok(value) => <usize as gimli::ReaderOffset>::from_u64(value),
        Err(e) => Err(e),
      },
    }
  }

  fn read_address(&mut self, address_size: u8) -> gimli::Result<u64> {
    println!("read_address");
    match address_size {
      1 => if self.data.len() - self.data_offset > 0 {
        self.data_offset += 1;
        Ok(self.data[self.data_offset - 1])
      } else {
        Err(Error::UnexpectedEof(self.offset_id()))
      }
      .map(u64::from),
      2 => {
        if self.data.len() - self.data_offset > 1 {
          self.data_offset += 2;
          Ok(
            self.endian.read_u16(
              self.data[self.data_offset - 2..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      }
      .map(u64::from),
      4 => {
        if self.data.len() - self.data_offset > 3 {
          self.data_offset += 4;
          Ok(
            self.endian.read_u32(
              self.data[self.data_offset - 4..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      }
      .map(u64::from),
      8 => {
        if self.data.len() - self.data_offset > 7 {
          self.data_offset += 8;
          Ok(
            self.endian.read_u64(
              self.data[self.data_offset - 8..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      }
      otherwise => Err(Error::UnsupportedAddressSize(otherwise)),
    }
  }

  fn read_length(&mut self, format: gimli::Format) -> gimli::Result<usize> {
    println!("read_length");

    match format {
      gimli::Format::Dwarf32 => match {
        if self.data.len() - self.data_offset > 3 {
          self.data_offset += 4;
          Ok(
            self.endian.read_u32(
              self.data[self.data_offset - 4..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      } {
        Ok(value) => Ok(<usize as gimli::ReaderOffset>::from_u32(value)),
        Err(e) => Err(e),
      },
      gimli::Format::Dwarf64 => match {
        if self.data.len() - self.data_offset > 7 {
          self.data_offset += 8;
          Ok(
            self.endian.read_u64(
              self.data[self.data_offset - 8..self.data_offset]
                .try_into()
                .unwrap(),
            ),
          )
        } else {
          Err(Error::UnexpectedEof(self.offset_id()))
        }
      } {
        Ok(value) => <usize as gimli::ReaderOffset>::from_u64(value),
        Err(e) => Err(e),
      },
    }
  }
}
