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
    self.endian
  }

  fn len(&self) -> usize {
    self.data.len() - self.data_offset
  }

  fn empty(&mut self) {
    self.data.clear();
    self.data_offset = 0;
  }

  fn truncate(&mut self, len: usize) -> Result<(), Error> {
    self.data.truncate(self.data_offset + len);
    Ok(())
  }

  fn offset_from(&self, base: &Self) -> usize {
    (self.section_offset + self.data_offset) - (base.section_offset + base.data_offset)
  }

  fn offset_id(&self) -> ReaderOffsetId {
    ReaderOffsetId(self.data_offset.try_into().unwrap())
  }

  fn lookup_offset_id(&self, id: ReaderOffsetId) -> Option<usize> {
    Some(id.0.try_into().unwrap())
  }

  fn find(&self, byte: u8) -> Result<usize, Error> {
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
    if self.data.len() < self.data_offset + len {
      Err(Error::UnexpectedEof(self.offset_id()))
    } else {
      self.data_offset += len;
      Ok(())
    }
  }

  fn split(&mut self, len: usize) -> Result<Self, Error> {
    if self.data.len() < self.data_offset + len {
      assert!(false);
      Err(Error::UnexpectedEof(self.offset_id()))
    } else {
      self.data_offset += len;

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
    Ok(self.data[self.data_offset..].into())
  }

  fn to_string(&self) -> Result<Cow<'_, str>, Error> {
    Ok(
      str::from_utf8(&self.data[self.data_offset..])
        .unwrap()
        .into(),
    )
  }

  fn to_string_lossy(&self) -> Result<Cow<'_, str>, Error> {
    Ok(
      str::from_utf8(&self.data[self.data_offset..])
        .unwrap()
        .into(),
    )
  }

  fn read_slice(&mut self, buf: &mut [u8]) -> Result<(), Error> {
    if self.len() >= 4 {
      let mut vec = vec![0; 4];
      vec.clone_from_slice(&self.data[self.data_offset..self.data_offset + 4]);
    }

    if self.data.len() < self.data_offset + buf.len() {
      Err(Error::UnexpectedEof(self.offset_id()))
    } else {
      for b in buf {
        *b = self.data[self.data_offset];
        self.data_offset += 1;
      }

      Ok(())
    }
  }
}
