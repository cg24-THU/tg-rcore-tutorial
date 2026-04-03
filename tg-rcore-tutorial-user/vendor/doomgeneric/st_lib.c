//
// Copyright(C) 1993-1996 Id Software, Inc.
// Copyright(C) 2005-2014 Simon Howard
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// DESCRIPTION:
//	The status bar widget code.
//


#include <stdio.h>
#include <ctype.h>

#include "deh_main.h"
#include "doomdef.h"

#include "z_zone.h"
#include "v_video.h"

#include "i_swap.h"
#include "i_system.h"

#include "w_wad.h"

#include "st_stuff.h"
#include "st_lib.h"
#include "r_local.h"


// in AM_map.c
extern boolean		automapactive; 




//
// Hack display negative frags.
//  Loads and store the stminus lump.
//
patch_t*		sttminus;
static int stlib_null_patch_warnings = 0;

static patch_t *STlib_CheckPatch(const char *func, patch_t *patch, int index)
{
    if (patch != NULL)
    {
        return patch;
    }

    if (stlib_null_patch_warnings < 8)
    {
        fprintf(stderr, "doom: %s null patch index=%d\n", func, index);
        ++stlib_null_patch_warnings;
    }

    return NULL;
}

static int STlib_CheckMultIconIndex(const char *func, st_multicon_t *mi, int index)
{
    if (index >= 0 && index < mi->count)
    {
        return 1;
    }

    if (stlib_null_patch_warnings < 8)
    {
        fprintf(stderr, "doom: %s bad index=%d count=%d\n", func, index, mi->count);
        ++stlib_null_patch_warnings;
    }

    return 0;
}

void STlib_init(void)
{
    sttminus = (patch_t *) W_CacheLumpName(DEH_String("STTMINUS"), PU_STATIC);
}


// ?
void
STlib_initNum
( st_number_t*		n,
  int			x,
  int			y,
  patch_t**		pl,
  int*			num,
  boolean*		on,
  int			width )
{
    n->x	= x;
    n->y	= y;
    n->oldnum	= 0;
    n->width	= width;
    n->num	= num;
    n->on	= on;
    n->p	= pl;
}


// 
// A fairly efficient way to draw a number
//  based on differences from the old number.
// Note: worth the trouble?
//
void
STlib_drawNum
( st_number_t*	n,
  boolean	refresh )
{

    int		numdigits = n->width;
    int		num = *n->num;
    patch_t *digit0 = STlib_CheckPatch("STlib_drawNum.base", n->p[0], 0);
    int		w;
    int		h;
    int		x = n->x;
    
    int		neg;

    if (digit0 == NULL)
    {
        return;
    }

    w = SHORT(digit0->width);
    h = SHORT(digit0->height);

    n->oldnum = *n->num;

    neg = num < 0;

    if (neg)
    {
	if (numdigits == 2 && num < -9)
	    num = -9;
	else if (numdigits == 3 && num < -99)
	    num = -99;
	
	num = -num;
    }

    // clear the area
    x = n->x - numdigits*w;

    if (n->y - ST_Y < 0)
	I_Error("drawNum: n->y - ST_Y < 0");

    V_CopyRect(x, n->y - ST_Y, st_backing_screen, w*numdigits, h, x, n->y);

    // if non-number, do not draw it
    if (num == 1994)
	return;

    x = n->x;

    // in the special case of 0, you draw 0
    if (!num)
    {
        patch_t *patch = STlib_CheckPatch("STlib_drawNum.zero", n->p[0], 0);

        if (patch != NULL)
        {
	    V_DrawPatch(x - w, n->y, patch);
        }
    }

    // draw the new number
    while (num && numdigits--)
    {
        int digit = num % 10;
        patch_t *patch = STlib_CheckPatch("STlib_drawNum.digit", n->p[digit], digit);

	x -= w;
        if (patch != NULL)
        {
	    V_DrawPatch(x, n->y, patch);
        }
	num /= 10;
    }

    // draw a minus sign if necessary
    if (neg)
    {
        patch_t *patch = STlib_CheckPatch("STlib_drawNum.minus", sttminus, -1);

        if (patch != NULL)
        {
	    V_DrawPatch(x - 8, n->y, patch);
        }
    }
}


//
void
STlib_updateNum
( st_number_t*		n,
  boolean		refresh )
{
    if (*n->on) STlib_drawNum(n, refresh);
}


//
void
STlib_initPercent
( st_percent_t*		p,
  int			x,
  int			y,
  patch_t**		pl,
  int*			num,
  boolean*		on,
  patch_t*		percent )
{
    STlib_initNum(&p->n, x, y, pl, num, on, 3);
    p->p = percent;
}




void
STlib_updatePercent
( st_percent_t*		per,
  int			refresh )
{
    if (refresh && *per->n.on)
    {
        patch_t *patch = STlib_CheckPatch("STlib_updatePercent", per->p, 0);

        if (patch != NULL)
        {
	    V_DrawPatch(per->n.x, per->n.y, patch);
        }
    }
    
    STlib_updateNum(&per->n, refresh);
}



void
STlib_initMultIcon
( st_multicon_t*	i,
  int			x,
  int			y,
  patch_t**		il,
  int                   count,
  int*			inum,
  boolean*		on )
{
    i->x	= x;
    i->y	= y;
    i->oldinum 	= -1;
    i->inum	= inum;
    i->on	= on;
    i->p	= il;
    i->count     = count;
}



void
STlib_updateMultIcon
( st_multicon_t*	mi,
  boolean		refresh )
{
    int			w;
    int			h;
    int			x;
    int			y;

    if (*mi->on && (mi->oldinum != *mi->inum || refresh) && (*mi->inum != -1))
    {
	if (mi->oldinum != -1
         && STlib_CheckMultIconIndex("STlib_updateMultIcon.old", mi, mi->oldinum))
	{
            patch_t *old_patch = STlib_CheckPatch("STlib_updateMultIcon.old", mi->p[mi->oldinum], mi->oldinum);

            if (old_patch != NULL)
            {
	        x = mi->x - SHORT(old_patch->leftoffset);
	        y = mi->y - SHORT(old_patch->topoffset);
	        w = SHORT(old_patch->width);
	        h = SHORT(old_patch->height);

	        if (y - ST_Y < 0)
		    I_Error("updateMultIcon: y - ST_Y < 0");

	        V_CopyRect(x, y-ST_Y, st_backing_screen, w, h, x, y);
            }
	}
        {
            patch_t *new_patch = NULL;

            if (STlib_CheckMultIconIndex("STlib_updateMultIcon.new", mi, *mi->inum))
            {
                new_patch = STlib_CheckPatch("STlib_updateMultIcon.new", mi->p[*mi->inum], *mi->inum);
            }

            if (new_patch != NULL)
            {
	        V_DrawPatch(mi->x, mi->y, new_patch);
            }
        }
	mi->oldinum = STlib_CheckMultIconIndex("STlib_updateMultIcon.store", mi, *mi->inum)
                    ? *mi->inum
                    : -1;
    }
}



void
STlib_initBinIcon
( st_binicon_t*		b,
  int			x,
  int			y,
  patch_t*		i,
  boolean*		val,
  boolean*		on )
{
    b->x	= x;
    b->y	= y;
    b->oldval	= false;
    b->val	= val;
    b->on	= on;
    b->p	= i;
}



void
STlib_updateBinIcon
( st_binicon_t*		bi,
  boolean		refresh )
{
    int			x;
    int			y;
    int			w;
    int			h;

    if (*bi->on
     && (bi->oldval != *bi->val || refresh))
    {
        patch_t *patch = STlib_CheckPatch("STlib_updateBinIcon", bi->p, 0);

        if (patch == NULL)
        {
            bi->oldval = *bi->val;
            return;
        }

	x = bi->x - SHORT(patch->leftoffset);
	y = bi->y - SHORT(patch->topoffset);
	w = SHORT(patch->width);
	h = SHORT(patch->height);

	if (y - ST_Y < 0)
	    I_Error("updateBinIcon: y - ST_Y < 0");

	if (*bi->val)
	    V_DrawPatch(bi->x, bi->y, patch);
	else
	    V_CopyRect(x, y-ST_Y, st_backing_screen, w, h, x, y);

	bi->oldval = *bi->val;
    }

}
