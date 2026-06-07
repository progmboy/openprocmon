# Module for locating the Windows Template Library (WTL).
#
# Customizable variables:
#   WTL_ROOT_DIR
#     This variable points to the Windows Template Library root directory.
#
# Read-only variables:
#   WTL_FOUND
#     Indicates that the library has been found.
#
#   WTL_INCLUDE_DIRS
#     Points to the Windows Template Library include directory.
#
#
# Copyright (c) 2012 Sergiu Dotenco
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

INCLUDE (FindPackageHandleStandardArgs)

FIND_PACKAGE (WTL NO_MODULE QUIET)

FIND_PATH (WTL_ROOT_DIR
  NAMES include/atlapp.h
  PATHS ENV WTLROOT
  HINTS ${WTL_INCLUDE_DIR}/..
  DOC "WTL root directory")

FIND_PATH (WTL_INCLUDE_DIR
  NAMES atlapp.h
  HINTS ${WTL_ROOT_DIR}
  PATH_SUFFIXES include
  DOC "WTL include directory")

IF (WTL_INCLUDE_DIR)
  SET (_WTL_VERSION_HEADER ${WTL_INCLUDE_DIR}/atlapp.h)

  IF (EXISTS ${_WTL_VERSION_HEADER})
    FILE (STRINGS ${_WTL_VERSION_HEADER} _WTL_VERSION_TMP REGEX
      "WTL version[ \t]+[0-9]+(\\.[0-9]+)?")

    STRING (REGEX REPLACE
      ".*WTL version[ \t]+([0-9]+(\\.[0-9]+)?)" "\\1" _WTL_VERSION_TMP
      ${_WTL_VERSION_TMP})

    STRING (REGEX REPLACE "([0-9]+)(\\.[0-9]+)?" "\\1" WTL_VERSION_MAJOR
      ${_WTL_VERSION_TMP})
    STRING (REGEX REPLACE "[0-9]+(\\.([0-9]+))?" "\\2" WTL_VERSION_MINOR
      ${_WTL_VERSION_TMP})

    IF (${WTL_VERSION_MINOR} STREQUAL "")
      SET (WTL_VERSION ${WTL_VERSION_MAJOR}.0)
    ELSE (${WTL_VERSION_MINOR} STREQUAL "")
      SET (WTL_VERSION ${WTL_VERSION_MAJOR}.${WTL_VERSION_MINOR})
    ENDIF (${WTL_VERSION_MINOR} STREQUAL "")

    SET (WTL_VERSION_COUNT 2)
  ENDIF (EXISTS ${_WTL_VERSION_HEADER})

  SET (WTL_INCLUDE_DIRS ${WTL_INCLUDE_DIR})
ENDIF (WTL_INCLUDE_DIR)

MARK_AS_ADVANCED (WTL_INCLUDE_DIR)

FIND_PACKAGE_HANDLE_STANDARD_ARGS (WTL REQUIRED_VARS WTL_ROOT_DIR
  WTL_INCLUDE_DIR VERSION_VAR WTL_VERSION)
