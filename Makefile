# Developer shortcuts. Builds the engine images (docker/engine.dockerfile) locally under the
# names the worker looks for, so a development setup needs no registry:
#
#   make engine-images-default            only the default version of each engine (quick)
#   make engine-images                    every version in docker/engine-versions.yml
#   make engine-image-nextflow            all versions of one engine
#   make engine-image ENGINE=nextflow VERSION=24.10.9
#   make engine-images-list               what would be built
#
# Variables:
#   RUNTIME   container CLI, default: docker if installed, else podman
#   REGISTRY  image name prefix, default ghcr.io/cubimedrub (what the worker pulls by default);
#             empty builds `macworp-nextflow:<version>`, then run the worker with
#             MACWORP_ENGINE_REGISTRY= (empty)

CATALOG  := docker/engine-versions.yml
REGISTRY ?= ghcr.io/cubimedrub
RUNTIME  ?= $(shell command -v docker >/dev/null 2>&1 && echo docker || echo podman)
IMAGE_PREFIX := $(if $(REGISTRY),$(REGISTRY)/,)

# One line per engine and version: `<engine> <target> <build arg> <version> <default|->`, read
# from the catalog (a fixed layout, see its header), so no YAML tool is needed.
CATALOG_ROWS := $(shell awk ' \
  /^[a-z]+:/ { engine = $$1; sub(":", "", engine); in_versions = 0 } \
  /^  target:/ { target = $$2 } \
  /^  build_arg:/ { build_arg = $$2 } \
  /^  default:/ { default_version = $$2; gsub("\"", "", default_version) } \
  /^  versions:/ { in_versions = 1; next } \
  in_versions && /^    - / { version = $$2; gsub("\"", "", version); \
    print engine ":" target ":" build_arg ":" version ":" (version == default_version ? "default" : "-") } \
  ' $(CATALOG))

# Fields of a row: engine, target, build arg, version, default marker
field = $(word $(2),$(subst :, ,$(1)))

.PHONY: engine-images engine-images-default engine-images-list engine-image

engine-images: $(foreach row,$(CATALOG_ROWS),engine-image-$(call field,$(row),1)-$(call field,$(row),4))

engine-images-default: $(foreach row,$(filter %:default,$(CATALOG_ROWS)),engine-image-$(call field,$(row),1)-$(call field,$(row),4))

engine-images-list:
	@$(foreach row,$(CATALOG_ROWS),echo "$(IMAGE_PREFIX)macworp-$(call field,$(row),2):$(call field,$(row),4)$(if $(filter default,$(call field,$(row),5)), (default, also :latest))";)

# Builds one engine and version: `make engine-image ENGINE=nextflow VERSION=25.10.9`
engine-image:
	@test -n "$(ENGINE)" -a -n "$(VERSION)" || { echo "usage: make engine-image ENGINE=<engine> VERSION=<version>"; exit 2; }
	@$(MAKE) --no-print-directory engine-image-$(ENGINE)-$(VERSION)

# engine-image-<engine>-<version> for every catalog row, and engine-image-<engine> for all
# versions of one engine. The default version also gets the tag `latest`, like CI does.
# Arguments: engine, target, build arg, version, default marker
define engine_image_rule
.PHONY: engine-image-$(1)-$(4)
engine-image-$(1)-$(4):
	$(RUNTIME) build -f docker/engine.dockerfile --target $(2) --build-arg $(3)=$(4) \
	  -t $(IMAGE_PREFIX)macworp-$(2):$(4) $(if $(filter default,$(5)),-t $(IMAGE_PREFIX)macworp-$(2):latest) docker
endef
$(foreach row,$(CATALOG_ROWS),$(eval $(call engine_image_rule,$(call field,$(row),1),$(call field,$(row),2),$(call field,$(row),3),$(call field,$(row),4),$(call field,$(row),5))))

ENGINES := $(sort $(foreach row,$(CATALOG_ROWS),$(call field,$(row),1)))
define engine_rule
.PHONY: engine-image-$(1)
engine-image-$(1): $(foreach row,$(filter $(1):%,$(CATALOG_ROWS)),engine-image-$(1)-$(call field,$(row),4))
endef
$(foreach engine,$(ENGINES),$(eval $(call engine_rule,$(engine))))
