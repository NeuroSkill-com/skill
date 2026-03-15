// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Apple Vision framework OCR — compiled ObjC with ARC.
// Exposes a single C function callable from Rust.

#import <Foundation/Foundation.h>
#import <Vision/Vision.h>
#import <CoreGraphics/CoreGraphics.h>
#include <stdlib.h>
#include <string.h>

/// Recognize text in an RGBA image using Apple Vision framework.
///
/// Runs on GPU / Apple Neural Engine via VNRecognizeTextRequest.
/// Uses fast recognition level (no language correction) for speed.
///
/// @param rgba_pixels  Raw RGBA pixel data (4 bytes per pixel).
/// @param width        Image width in pixels.
/// @param height       Image height in pixels.
/// @param out_len      On success, receives the length of the returned string.
/// @return             A malloc'd UTF-8 C string with extracted text (lines
///                     separated by '\n'), or NULL if no text was found.
///                     The caller must free() the returned pointer.
char *apple_vision_ocr(const uint8_t *rgba_pixels,
                       uint32_t width,
                       uint32_t height,
                       uint32_t *out_len)
{
    if (!rgba_pixels || width == 0 || height == 0) return NULL;

    @autoreleasepool {
        // Create CGImage from raw RGBA pixels
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL, rgba_pixels, (size_t)width * height * 4, NULL);

        CGImageRef cgImage = CGImageCreate(
            width, height,
            8,                          // bitsPerComponent
            32,                         // bitsPerPixel
            width * 4,                  // bytesPerRow
            colorSpace,
            kCGImageAlphaPremultipliedLast | kCGBitmapByteOrderDefault,
            provider,
            NULL,                       // decode
            false,                      // shouldInterpolate
            kCGRenderingIntentDefault);

        CGColorSpaceRelease(colorSpace);
        CGDataProviderRelease(provider);

        if (!cgImage) return NULL;

        // Create and configure the text recognition request
        VNRecognizeTextRequest *request = [[VNRecognizeTextRequest alloc] init];
        request.recognitionLevel = VNRequestTextRecognitionLevelFast;
        request.usesLanguageCorrection = NO;

        // Run the request
        VNImageRequestHandler *handler = [[VNImageRequestHandler alloc]
            initWithCGImage:cgImage options:@{}];

        NSError *error = nil;
        BOOL success = [handler performRequests:@[request] error:&error];
        CGImageRelease(cgImage);

        if (!success || !request.results || request.results.count == 0) {
            return NULL;
        }

        // Collect recognized text lines
        NSMutableString *result = [NSMutableString string];
        for (VNRecognizedTextObservation *obs in request.results) {
            NSArray<VNRecognizedText *> *candidates = [obs topCandidates:1];
            if (candidates.count > 0) {
                NSString *text = candidates[0].string;
                if (text.length > 0) {
                    if (result.length > 0) [result appendString:@"\n"];
                    [result appendString:text];
                }
            }
        }

        if (result.length == 0) return NULL;

        // Convert to a malloc'd C string
        const char *utf8 = [result UTF8String];
        size_t len = strlen(utf8);
        char *out = (char *)malloc(len + 1);
        if (!out) return NULL;
        memcpy(out, utf8, len + 1);

        if (out_len) *out_len = (uint32_t)len;
        return out;
    }
}
