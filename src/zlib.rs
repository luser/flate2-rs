//! ZLIB compression and decompression of streams

use std::io::prelude::*;
use std::io;
use std::mem;

use bufreader::BufReader;
use zio;
use {Compress, Decompress};

/// A ZLIB encoder, or compressor.
///
/// This structure implements a `Write` interface and takes a stream of
/// uncompressed data, writing the compressed data to the wrapped writer.
pub struct EncoderWriter<W: Write> {
    inner: zio::Writer<W, Compress>,
}

/// A ZLIB encoder, or compressor.
///
/// This structure implements a `Read` interface and will read uncompressed
/// data from an underlying stream and emit a stream of compressed data.
pub struct EncoderReader<R> {
    inner: EncoderReaderBuf<BufReader<R>>,
}

/// A ZLIB encoder, or compressor.
///
/// This structure implements a `BufRead` interface and will read uncompressed
/// data from an underlying stream and emit a stream of compressed data.
pub struct EncoderReaderBuf<R> {
    obj: R,
    data: Compress,
}

/// A ZLIB decoder, or decompressor.
///
/// This structure implements a `Read` interface and takes a stream of
/// compressed data as input, providing the decompressed data when read from.
pub struct DecoderReader<R> {
    inner: DecoderReaderBuf<BufReader<R>>,
}

/// A ZLIB decoder, or decompressor.
///
/// This structure implements a `BufRead` interface and takes a stream of
/// compressed data as input, providing the decompressed data when read from.
pub struct DecoderReaderBuf<R> {
    obj: R,
    data: Decompress,
}

/// A ZLIB decoder, or decompressor.
///
/// This structure implements a `Write` and will emit a stream of decompressed
/// data when fed a stream of compressed data.
pub struct DecoderWriter<W: Write> {
    inner: zio::Writer<W, Decompress>,
}

impl<W: Write> EncoderWriter<W> {
    /// Creates a new encoder which will write compressed data to the stream
    /// given at the given compression level.
    ///
    /// When this encoder is dropped or unwrapped the final pieces of data will
    /// be flushed.
    pub fn new(w: W, level: ::Compression) -> EncoderWriter<W> {
        EncoderWriter {
            inner: zio::Writer::new(w, Compress::new(level, true)),
        }
    }

    /// Resets the state of this encoder entirely, swapping out the output
    /// stream for another.
    ///
    /// This function will finish encoding the current stream into the current
    /// output stream before swapping out the two output streams. If the stream
    /// cannot be finished an error is returned.
    ///
    /// After the current stream has been finished, this will reset the internal
    /// state of this encoder and replace the output stream with the one
    /// provided, returning the previous output stream. Future data written to
    /// this encoder will be the compressed into the stream `w` provided.
    pub fn reset(&mut self, w: W) -> io::Result<W> {
        try!(self.inner.finish());
        self.inner.data.reset();
        Ok(self.inner.replace(w))
    }

    /// Consumes this encoder, flushing the output stream.
    ///
    /// This will flush the underlying data stream and then return the contained
    /// writer if the flush succeeded.
    pub fn finish(mut self) -> io::Result<W> {
        try!(self.inner.finish());
        Ok(self.inner.into_inner())
    }
}

impl<W: Write> Write for EncoderWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<R: Read> EncoderReader<R> {
    /// Creates a new encoder which will read uncompressed data from the given
    /// stream and emit the compressed stream.
    pub fn new(r: R, level: ::Compression) -> EncoderReader<R> {
        EncoderReader {
            inner: EncoderReaderBuf::new(BufReader::new(r), level),
        }
    }

    /// Resets the state of this encoder entirely, swapping out the input
    /// stream for another.
    ///
    /// This function will reset the internal state of this encoder and replace
    /// the input stream with the one provided, returning the previous input
    /// stream. Future data read from this encoder will be the compressed
    /// version of `r`'s data.
    pub fn reset(&mut self, r: R) -> R {
        self.inner.data.reset();
        self.inner.obj.reset(r)
    }

    /// Acquires a reference to the underlying stream
    pub fn get_ref(&self) -> &R {
        self.inner.get_ref().get_ref()
    }

    /// Acquires a mutable reference to the underlying stream
    ///
    /// Note that mutation of the stream may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        self.inner.get_mut().get_mut()
    }

    /// Consumes this encoder, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.inner.into_inner().into_inner()
    }
}

impl<R: Read> Read for EncoderReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: BufRead> EncoderReaderBuf<R> {
    /// Creates a new encoder which will read uncompressed data from the given
    /// stream and emit the compressed stream.
    pub fn new(r: R, level: ::Compression) -> EncoderReaderBuf<R> {
        EncoderReaderBuf {
            obj: r,
            data: Compress::new(level, true),
        }
    }

    /// Resets the state of this encoder entirely, swapping out the input
    /// stream for another.
    ///
    /// This function will reset the internal state of this encoder and replace
    /// the input stream with the one provided, returning the previous input
    /// stream. Future data read from this encoder will be the compressed
    /// version of `r`'s data.
    pub fn reset(&mut self, r: R) -> R {
        self.data.reset();
        mem::replace(&mut self.obj, r)
    }

    /// Acquires a reference to the underlying stream
    pub fn get_ref(&self) -> &R {
        &self.obj
    }

    /// Acquires a mutable reference to the underlying stream
    ///
    /// Note that mutation of the stream may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.obj
    }

    /// Consumes this encoder, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.obj
    }
}

impl<R: BufRead> Read for EncoderReaderBuf<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        zio::read(&mut self.obj, &mut self.data, buf)
    }
}

impl<R: Read> DecoderReader<R> {
    /// Creates a new decoder which will decompress data read from the given
    /// stream.
    pub fn new(r: R) -> DecoderReader<R> {
        DecoderReader::new_with_buf(r, vec![0; 32 * 1024])
    }

    /// Same as `new`, but the intermediate buffer for data is specified.
    ///
    /// Note that the specified buffer will only be used up to its current
    /// length. The buffer's capacity will also not grow over time.
    pub fn new_with_buf(r: R, buf: Vec<u8>) -> DecoderReader<R> {
        DecoderReader {
            inner: DecoderReaderBuf::new(BufReader::with_buf(buf, r)),
        }
    }

    /// Resets the state of this decoder entirely, swapping out the input
    /// stream for another.
    ///
    /// This will reset the internal state of this decoder and replace the
    /// input stream with the one provided, returning the previous input
    /// stream. Future data read from this decoder will be the decompressed
    /// version of `r`'s data.
    pub fn reset(&mut self, r: R) -> R {
        self.inner.data = Decompress::new(true);
        self.inner.obj.reset(r)
    }

    /// Acquires a reference to the underlying stream
    pub fn get_ref(&self) -> &R {
        self.inner.get_ref().get_ref()
    }

    /// Acquires a mutable reference to the underlying stream
    ///
    /// Note that mutation of the stream may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        self.inner.get_mut().get_mut()
    }

    /// Consumes this decoder, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.inner.into_inner().into_inner()
    }

    /// Returns the number of bytes that the decompressor has consumed.
    ///
    /// Note that this will likely be smaller than what the decompressor
    /// actually read from the underlying stream due to buffering.
    pub fn total_in(&self) -> u64 {
        self.inner.total_in()
    }

    /// Returns the number of bytes that the decompressor has produced.
    pub fn total_out(&self) -> u64 {
        self.inner.total_out()
    }
}

impl<R: Read> Read for DecoderReader<R> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        self.inner.read(into)
    }
}

impl<R: BufRead> DecoderReaderBuf<R> {
    /// Creates a new decoder which will decompress data read from the given
    /// stream.
    pub fn new(r: R) -> DecoderReaderBuf<R> {
        DecoderReaderBuf {
            obj: r,
            data: Decompress::new(true),
        }
    }

    /// Resets the state of this decoder entirely, swapping out the input
    /// stream for another.
    ///
    /// This will reset the internal state of this decoder and replace the
    /// input stream with the one provided, returning the previous input
    /// stream. Future data read from this decoder will be the decompressed
    /// version of `r`'s data.
    pub fn reset(&mut self, r: R) -> R {
        self.data = Decompress::new(true);
        mem::replace(&mut self.obj, r)
    }

    /// Acquires a reference to the underlying stream
    pub fn get_ref(&self) -> &R {
        &self.obj
    }

    /// Acquires a mutable reference to the underlying stream
    ///
    /// Note that mutation of the stream may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.obj
    }

    /// Consumes this decoder, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.obj
    }

    /// Returns the number of bytes that the decompressor has consumed.
    ///
    /// Note that this will likely be smaller than what the decompressor
    /// actually read from the underlying stream due to buffering.
    pub fn total_in(&self) -> u64 {
        self.data.total_in()
    }

    /// Returns the number of bytes that the decompressor has produced.
    pub fn total_out(&self) -> u64 {
        self.data.total_out()
    }
}

impl<R: BufRead> Read for DecoderReaderBuf<R> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        zio::read(&mut self.obj, &mut self.data, into)
    }
}

impl<W: Write> DecoderWriter<W> {
    /// Creates a new decoder which will write uncompressed data to the stream.
    ///
    /// When this decoder is dropped or unwrapped the final pieces of data will
    /// be flushed.
    pub fn new(w: W) -> DecoderWriter<W> {
        DecoderWriter {
            inner: zio::Writer::new(w, Decompress::new(true)),
        }
    }

    /// Resets the state of this decoder entirely, swapping out the output
    /// stream for another.
    ///
    /// This will reset the internal state of this decoder and replace the
    /// output stream with the one provided, returning the previous output
    /// stream. Future data written to this decoder will be decompressed into
    /// the output stream `w`.
    pub fn reset(&mut self, w: W) -> io::Result<W> {
        try!(self.inner.finish());
        self.inner.data = Decompress::new(true);
        Ok(self.inner.replace(w))
    }

    /// Consumes this encoder, flushing the output stream.
    ///
    /// This will flush the underlying data stream and then return the contained
    /// writer if the flush succeeded.
    pub fn finish(mut self) -> io::Result<W> {
        try!(self.inner.finish());
        Ok(self.inner.into_inner())
    }

    /// Returns the number of bytes that the decompressor has consumed for
    /// decompression.
    ///
    /// Note that this will likely be smaller than the number of bytes
    /// successfully written to this stream due to internal buffering.
    pub fn total_in(&self) -> u64 {
        self.inner.data.total_in()
    }

    /// Returns the number of bytes that the decompressor has written to its
    /// output stream.
    pub fn total_out(&self) -> u64 {
        self.inner.data.total_out()
    }
}

impl<W: Write> Write for DecoderWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::io::prelude::*;
    use std::io;

    use rand::{thread_rng, Rng};

    use zlib::{EncoderWriter, EncoderReader, DecoderReader, DecoderWriter};
    use Compression::Default;

    #[test]
    fn roundtrip() {
        let mut real = Vec::new();
        let mut w = EncoderWriter::new(Vec::new(), Default);
        let v = thread_rng().gen_iter::<u8>().take(1024).collect::<Vec<_>>();
        for _ in 0..200 {
            let to_write = &v[..thread_rng().gen_range(0, v.len())];
            real.extend(to_write.iter().map(|x| *x));
            w.write_all(to_write).unwrap();
        }
        let result = w.finish().unwrap();
        let mut r = DecoderReader::new(&result[..]);
        let mut ret = Vec::new();
        r.read_to_end(&mut ret).unwrap();
        assert!(ret == real);
    }

    #[test]
    fn drop_writes() {
        let mut data = Vec::new();
        EncoderWriter::new(&mut data, Default).write_all(b"foo").unwrap();
        let mut r = DecoderReader::new(&data[..]);
        let mut ret = Vec::new();
        r.read_to_end(&mut ret).unwrap();
        assert!(ret == b"foo");
    }

    #[test]
    fn total_in() {
        let mut real = Vec::new();
        let mut w = EncoderWriter::new(Vec::new(), Default);
        let v = thread_rng().gen_iter::<u8>().take(1024).collect::<Vec<_>>();
        for _ in 0..200 {
            let to_write = &v[..thread_rng().gen_range(0, v.len())];
            real.extend(to_write.iter().map(|x| *x));
            w.write_all(to_write).unwrap();
        }
        let mut result = w.finish().unwrap();

        let result_len = result.len();

        for _ in 0..200 {
            result.extend(v.iter().map(|x| *x));
        }

        let mut r = DecoderReader::new(&result[..]);
        let mut ret = Vec::new();
        r.read_to_end(&mut ret).unwrap();
        assert!(ret == real);
        assert_eq!(r.total_in(), result_len as u64);
    }

    #[test]
    fn roundtrip2() {
        let v = thread_rng()
                    .gen_iter::<u8>()
                    .take(1024 * 1024)
                    .collect::<Vec<_>>();
        let mut r = DecoderReader::new(EncoderReader::new(&v[..], Default));
        let mut ret = Vec::new();
        r.read_to_end(&mut ret).unwrap();
        assert_eq!(ret, v);
    }

    #[test]
    fn roundtrip3() {
        let v = thread_rng()
                    .gen_iter::<u8>()
                    .take(1024 * 1024)
                    .collect::<Vec<_>>();
        let mut w = EncoderWriter::new(DecoderWriter::new(Vec::new()), Default);
        w.write_all(&v).unwrap();
        let w = w.finish().unwrap().finish().unwrap();
        assert!(w == v);
    }

    #[test]
    fn reset_decoder() {
        let v = thread_rng()
                    .gen_iter::<u8>()
                    .take(1024 * 1024)
                    .collect::<Vec<_>>();
        let mut w = EncoderWriter::new(Vec::new(), Default);
        w.write_all(&v).unwrap();
        let data = w.finish().unwrap();

        {
            let (mut a, mut b, mut c) = (Vec::new(), Vec::new(), Vec::new());
            let mut r = DecoderReader::new(&data[..]);
            r.read_to_end(&mut a).unwrap();
            r.reset(&data);
            r.read_to_end(&mut b).unwrap();

            let mut r = DecoderReader::new(&data[..]);
            r.read_to_end(&mut c).unwrap();
            assert!(a == b && b == c && c == v);
        }

        {
            let mut w = DecoderWriter::new(Vec::new());
            w.write_all(&data).unwrap();
            let a = w.reset(Vec::new()).unwrap();
            w.write_all(&data).unwrap();
            let b = w.finish().unwrap();

            let mut w = DecoderWriter::new(Vec::new());
            w.write_all(&data).unwrap();
            let c = w.finish().unwrap();
            assert!(a == b && b == c && c == v);
        }
    }

    #[test]
    fn bad_input() {
        // regress tests: previously caused a panic on drop
        let mut out: Vec<u8> = Vec::new();
        let data: Vec<u8> = (0..255).cycle().take(1024).collect();
        let mut w = DecoderWriter::new(&mut out);
        match w.write_all(&data[..]) {
            Ok(_) => panic!("Expected an error to be returned!"),
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidInput),
        }
    }

    #[test]
    fn qc_reader() {
        ::quickcheck::quickcheck(test as fn(_) -> _);

        fn test(v: Vec<u8>) -> bool {
            let mut r = DecoderReader::new(EncoderReader::new(&v[..], Default));
            let mut v2 = Vec::new();
            r.read_to_end(&mut v2).unwrap();
            v == v2
        }
    }

    #[test]
    fn qc_writer() {
        ::quickcheck::quickcheck(test as fn(_) -> _);

        fn test(v: Vec<u8>) -> bool {
            let mut w = EncoderWriter::new(DecoderWriter::new(Vec::new()), Default);
            w.write_all(&v).unwrap();
            v == w.finish().unwrap().finish().unwrap()
        }
    }
}
