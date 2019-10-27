use rustc::middle::lang_items::PanicLocationLangItem;
use rustc::mir::interpret::{Pointer, PointerArithmetic, Scalar};
use rustc::ty::subst::Subst;
use rustc_target::abi::{LayoutOf, Size};
use syntax_pos::{Span, Symbol};

use crate::interpret::{MemoryKind, MPlaceTy, intrinsics::{InterpCx, InterpResult, Machine}};

impl<'mir, 'tcx, M: Machine<'mir, 'tcx>> InterpCx<'mir, 'tcx, M> {
    /// Walks up the callstack from the intrinsic's callsite, searching for the first frame which is
    /// not `#[track_caller]`. Returns the (passed) span of the intrinsic's callsite if the first
    /// frame in the stack is untracked so that we can display the callsite of the intrinsic within
    /// that function.
    crate fn find_closest_untracked_caller_location(
        &self,
        intrinsic_loc: Span,
    ) -> Span {
        debug!("finding closest untracked caller relative to {:?}", intrinsic_loc);

        let mut caller_span = intrinsic_loc;
        for next_caller in self.stack.iter().rev() {
            if !next_caller.instance.def.requires_caller_location(*self.tcx) {
                return caller_span;
            }
            caller_span = next_caller.span;
        }

        intrinsic_loc
    }

    /// Allocate a `const core::panic::Location` with the provided filename and line/column numbers.
    pub fn alloc_caller_location(
        &mut self,
        filename: Symbol,
        line: u32,
        col: u32,
    ) -> InterpResult<'tcx, MPlaceTy<'tcx, M::PointerTag>> {
        let line = Scalar::from_u32(line);
        let col = Scalar::from_u32(col);

        let ptr_size = self.pointer_size();
        let u32_size = Size::from_bits(32);

        // we can't use TyCtxt::caller_location_ty because that returns `&'static Location`
        let static_subst = self.tcx.mk_substs([self.tcx.lifetimes.re_static.into()].iter());
        let loc_ty = self.tcx.type_of(self.tcx.require_lang_item(PanicLocationLangItem, None))
            .subst(*self.tcx, static_subst);
        let loc_layout = self.layout_of(loc_ty)?;

        // we have all our sizes and layouts, start allocating
        let file_alloc = self.tcx.allocate_bytes(filename.as_str().as_bytes());
        let file_ptr = Pointer::new(file_alloc, Size::ZERO);
        let file = Scalar::Ptr(self.tag_static_base_pointer(file_ptr));
        let file_len = Scalar::from_uint(filename.as_str().len() as u128, ptr_size);

        let location = self.allocate(loc_layout, MemoryKind::Stack);

        // now that the struct is allocated, we need to get field offsets
        let file_out = self.mplace_field(location, 0)?;
        let file_ptr_out = self.force_ptr(self.mplace_field(file_out, 0)?.ptr)?;
        let file_len_out = self.force_ptr(self.mplace_field(file_out, 1)?.ptr)?;
        let line_out = self.force_ptr(self.mplace_field(location, 1)?.ptr)?;
        let col_out = self.force_ptr(self.mplace_field(location, 2)?.ptr)?;

        let layout = &self.tcx.data_layout;
        // We just allocated this, so we can skip the bounds checks.
        let alloc = self.memory.get_raw_mut(file_ptr_out.alloc_id)?;

        // finally write into the field offsets
        alloc.write_scalar(layout, file_ptr_out, file.into(), ptr_size)?;
        alloc.write_scalar(layout, file_len_out, file_len.into(), ptr_size)?;
        alloc.write_scalar(layout, line_out, line.into(), u32_size)?;
        alloc.write_scalar(layout, col_out, col.into(), u32_size)?;

        Ok(location)
    }
}
