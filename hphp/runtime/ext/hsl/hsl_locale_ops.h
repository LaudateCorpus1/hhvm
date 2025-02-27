/*
   +----------------------------------------------------------------------+
   | HipHop for PHP                                                       |
   +----------------------------------------------------------------------+
   | Copyright (c) 2010-present Facebook, Inc. (http://www.facebook.com)  |
   | Copyright (c) 1997-2010 The PHP Group                                |
   +----------------------------------------------------------------------+
   | This source file is subject to version 3.01 of the PHP license,      |
   | that is bundled with this package in the file LICENSE, and is        |
   | available through the world-wide-web at the following url:           |
   | http://www.php.net/license/3_01.txt                                  |
   | If you did not receive a copy of the PHP license and are unable to   |
   | obtain it through the world-wide-web, please send a note to          |
   | license@php.net so we can mail you a copy immediately.               |
   +----------------------------------------------------------------------+
*/

#pragma once

#include "hphp/runtime/ext/hsl/ext_hsl_locale.h"

namespace HPHP {
  struct HSLLocale::Ops {
    virtual ~Ops() {}
    virtual int64_t strlen(const String&) const = 0;
    virtual String uppercase(const String&) const = 0;
    virtual String lowercase(const String&) const = 0;
    virtual String foldcase(const String&) const = 0;
    virtual int64_t strcoll(const String&, const String&) const = 0;
    virtual int64_t strcasecmp(const String&, const String&) const = 0;

    // Special-case each of these instead of adding `substrcoll`/`substrcasecmp`
    // as:
    // - these are all that are needed for the HSL
    // - they can be more performant, especially for the HSL; for example,
    //   calculating offsets and lengths may require an encoding conversion
    //   which we're going to have to do here anyway, let's not do it twice
    virtual bool starts_with(const String& str, const String& prefix) const = 0;
    virtual bool starts_with_ci(const String& str, const String& prefix) const = 0;
    virtual bool ends_with(const String& str, const String& prefix) const = 0;
    virtual bool ends_with_ci(const String& str, const String& prefix) const = 0;

    virtual String strip_prefix(const String& str, const String& prefix) const = 0;
    virtual String strip_suffix(const String& str, const String& suffix) const = 0;

    virtual int64_t strpos(const String& haystack, const String& needle, int64_t offset) const = 0;
    virtual int64_t strrpos(const String& haystack, const String& needle, int64_t offset) const = 0;
    virtual int64_t stripos(const String& haystack, const String& needle, int64_t offset) const = 0;
    virtual int64_t strripos(const String& haystack, const String& needle, int64_t offset) const = 0;

    virtual Array chunk(const String&, int64_t chunk_size) const = 0;
    virtual String slice(const String& str, int64_t offset, int64_t length) const = 0;
    virtual String splice(const String& str, const String& replacement, int64_t offset, int64_t length) const = 0;
    virtual Array split(const String& str, const String& delimiter, int64_t limit = -1) const = 0;

    virtual String reverse(const String& str) const = 0;

    virtual String pad_left(const String& str, int64_t len, const String& pad) const = 0;
    virtual String pad_right(const String& str, int64_t len, const String& pad) const = 0;

    enum class TrimSides: uint8_t {
      LEFT = 1,
      RIGHT = 2,
      BOTH = LEFT | RIGHT
    };

    virtual String trim(const String& str, TrimSides sides) const = 0;
    virtual String trim(const String& str, const String& what, TrimSides sides) const = 0;

    virtual String replace(const String& haystack, const String& needle, const String& replacement) const = 0;
    virtual String replace_ci(const String& haystack, const String& needle, const String& replacement) const = 0;
    virtual String replace_every(const String& haystack, const Array& replacements) const = 0;
    virtual String replace_every_ci(const String& haystack, const Array& replacements) const = 0;
    virtual String replace_every_nonrecursive(const String& haystack, const Array& replacements) const = 0;
    virtual String replace_every_nonrecursive_ci(const String& haystack, const Array& replacements) const = 0;
  };
} // namespace HPHP
