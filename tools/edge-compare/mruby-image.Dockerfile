# The reference mruby at an arbitrary commit, for `tools/edge_compare.sh` (its `mrbc` compiles
# the test files the comparison with mruby/edge runs). Local only (not pushed).
#
#   tools/edge-compare/build-image.sh [commit]     (default: the commit mruby/edge's suite copied)
#
# The same steps as kishima/mruby:<tag> (the book's `ref/mruby_containers/
# rake_nobison_install_bookworm.Dockerfile`, which `docker history kishima/mruby:4.1.0-rc2`
# matches layer for layer), with one difference: the second stage declares `ARG MRUBY_VER`
# again, so the source tree kept in the image is the commit that was built (in the original the
# argument is not in scope there and the tree is the default branch's head).
FROM ruby:3.2.2-bookworm AS build
ARG MRUBY_VER

RUN apt-get update && apt-get install -y \
    build-essential \
    git

RUN git clone https://github.com/mruby/mruby.git /usr/src/mruby
WORKDIR /usr/src/mruby
RUN git checkout $MRUBY_VER
RUN rake && rake install

FROM ruby:3.2.2-bookworm
ARG MRUBY_VER
RUN apt-get update && apt-get install -y \
    build-essential \
    git
RUN git clone https://github.com/mruby/mruby.git /usr/src/mruby
WORKDIR /usr/src/mruby
RUN git checkout $MRUBY_VER
COPY --from=build /usr/local/ /usr/local
RUN mruby --version && git rev-parse HEAD
