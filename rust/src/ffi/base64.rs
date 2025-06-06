/* Copyright (C) 2021-2024 Open Information Security Foundation
 *
 * You can copy, redistribute or modify this Program under the terms of
 * the GNU General Public License version 2 as published by the Free
 * Software Foundation.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * version 2 along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA
 * 02110-1301, USA.
 */

use crate::utils::base64::{decode_rfc2045, decode_rfc4648, get_decoded_buffer_size, Decoder};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    Engine,
};
use libc::c_ulong;
use std::os::raw::c_uchar;

#[repr(C)]
#[allow(non_camel_case_types)]
pub enum SCBase64ReturnCode {
    SC_BASE64_OK = 0,
    SC_BASE64_INVALID_ARG,
    SC_BASE64_OVERFLOW,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SCBase64Mode {
    /* If the following strings were to be passed to the decoder with RFC2045 mode,
     * the results would be as follows. See the unittest B64TestVectorsRFC2045 in
     * src/util-base64.c
     *
     * BASE64("") = ""
     * BASE64("f") = "Zg=="
     * BASE64("fo") = "Zm8="
     * BASE64("foo") = "Zm9v"
     * BASE64("foob") = "Zm9vYg=="
     * BASE64("fooba") = "Zm9vYmE="
     * BASE64("foobar") = "Zm9vYmFy"
     * BASE64("foobar") = "Zm 9v Ym Fy"   <-- Notice how the spaces are ignored
     * BASE64("foobar") = "Zm$9vYm.Fy"    # According to RFC 2045, All line breaks or *other
     * characters* not found in base64 alphabet must be ignored by decoding software
     * */
    SCBase64ModeRFC2045 = 0, /* SPs are allowed during transfer but must be skipped by Decoder */
    SCBase64ModeStrict,
    /* If the following strings were to be passed to the decoder with RFC4648 mode,
     * the results would be as follows. See the unittest B64TestVectorsRFC4648 in
     * src/util-base64.c
     *
     * BASE64("") = ""
     * BASE64("f") = "Zg=="
     * BASE64("fo") = "Zm8="
     * BASE64("foo") = "Zm9v"
     * BASE64("foob") = "Zm9vYg=="
     * BASE64("fooba") = "Zm9vYmE="
     * BASE64("foobar") = "Zm9vYmFy"
     * BASE64("f") = "Zm 9v Ym Fy"   <-- Notice how the processing stops once space is encountered
     * BASE64("f") = "Zm$9vYm.Fy"    <-- Notice how the processing stops once an invalid char is
     * encountered
     * */
    SCBase64ModeRFC4648, /* reject the encoded data if it contains characters outside the base alphabet */

    /// Standard base64 without padding, and strict about it.
    SCBase64ModeNoPad,

    /// Standard base64 with optional padding: decode only.
    SCBase64ModePadOpt,
}

#[no_mangle]
pub unsafe extern "C" fn SCBase64DecodeBufferSize(input_len: u32) -> u32 {
    return get_decoded_buffer_size(input_len);
}

/// Base64 decode a buffer.
///
/// This method exposes the Rust base64 decoder to C and should not be called from
/// Rust code.
///
/// It allows decoding in the modes described by ``SCBase64Mode`` enum.
#[no_mangle]
pub unsafe extern "C" fn SCBase64Decode(
    input: *const u8, len: usize, mode: SCBase64Mode, output: *mut u8,
) -> u32 {
    if input.is_null() || len == 0 {
        return 0;
    }

    let in_vec = build_slice!(input, len);
    let out_vec = std::slice::from_raw_parts_mut(output, len);
    let mut num_decoded: u32 = 0;
    let mut decoder = Decoder::new();
    match mode {
        SCBase64Mode::SCBase64ModeRFC2045 => {
            if decode_rfc2045(&mut decoder, in_vec, out_vec, &mut num_decoded).is_err() {
                debug_validate_bug_on!(num_decoded >= len as u32);
                return num_decoded;
            }
        }
        SCBase64Mode::SCBase64ModeRFC4648 => {
            if decode_rfc4648(&mut decoder, in_vec, out_vec, &mut num_decoded).is_err() {
                debug_validate_bug_on!(num_decoded >= len as u32);
                return num_decoded;
            }
        }
        SCBase64Mode::SCBase64ModeStrict => {
            if let Ok(decoded_len) = STANDARD.decode_slice(in_vec, out_vec) {
                num_decoded = decoded_len as u32;
            }
        }
        SCBase64Mode::SCBase64ModeNoPad => {
            if let Ok(decoded_len) = STANDARD_NO_PAD.decode_slice(in_vec, out_vec) {
                num_decoded = decoded_len as u32;
            }
        }
        SCBase64Mode::SCBase64ModePadOpt => {
            let config = base64::engine::GeneralPurposeConfig::new()
                .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent);
            let decoder = base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, config);
            if let Ok(decoded_len) = decoder.decode_slice(in_vec, out_vec) {
                num_decoded = decoded_len as u32;
            }
        }
    }

    debug_validate_bug_on!(num_decoded >= len as u32);
    return num_decoded;
}

/// Base64 encode a buffer with a provided mode.
///
/// This method exposes the Rust base64 encoder to C and should not be called from
/// Rust code.
///
/// The output parameter must be an allocated buffer of at least the size returned
/// from SCBase64EncodeBufferSize for the input_len, and this length must be provided
/// in the output_len variable.
#[no_mangle]
pub unsafe extern "C" fn SCBase64EncodeWithMode(
    input: *const u8, input_len: c_ulong, output: *mut c_uchar, output_len: *mut c_ulong,
    mode: SCBase64Mode,
) -> SCBase64ReturnCode {
    if input.is_null() || output.is_null() || output_len.is_null() {
        return SCBase64ReturnCode::SC_BASE64_INVALID_ARG;
    }
    let input = std::slice::from_raw_parts(input, input_len as usize);
    let encoded = match mode {
        SCBase64Mode::SCBase64ModeNoPad => STANDARD_NO_PAD.encode(input),
        _ => STANDARD.encode(input),
    };
    if encoded.len() + 1 > *output_len as usize {
        return SCBase64ReturnCode::SC_BASE64_OVERFLOW;
    }
    let output = std::slice::from_raw_parts_mut(&mut *output, *output_len as usize);
    output[0..encoded.len()].copy_from_slice(encoded.as_bytes());
    output[encoded.len()] = 0;
    *output_len = encoded.len() as c_ulong;
    SCBase64ReturnCode::SC_BASE64_OK
}

/// Base64 encode a buffer.
///
/// This method exposes the Rust base64 encoder to C and should not be called from
/// Rust code.
///
/// The output parameter must be an allocated buffer of at least the size returned
/// from SCBase64EncodeBufferSize for the input_len, and this length must be provided
/// in the output_len variable.
#[no_mangle]
pub unsafe extern "C" fn SCBase64Encode(
    input: *const u8, input_len: c_ulong, output: *mut c_uchar, output_len: *mut c_ulong,
) -> SCBase64ReturnCode {
    SCBase64EncodeWithMode(
        input,
        input_len,
        output,
        output_len,
        SCBase64Mode::SCBase64ModeStrict,
    )
}

/// Ratio of output bytes to input bytes for Base64 Encoding is 4:3, hence the
/// required output bytes are 4 * ceil(input_len / 3) and an additional byte for
/// storing the NULL pointer.
#[no_mangle]
pub extern "C" fn SCBase64EncodeBufferSize(len: c_ulong) -> c_ulong {
    (4 * ((len) + 2) / 3) + 1
}
